use tree_sitter::{Node, Parser};

#[derive(Debug)]
pub struct SourceChunk {
    pub label: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub source: String,
    pub columns: usize,
    pub rows: usize,
}

pub struct Chunker {
    parser: Parser,
    max_rows: usize,
    wrap_column: usize,
}

impl Chunker {
    pub fn new(max_rows: usize, wrap_column: usize) -> Result<Self, tree_sitter::LanguageError> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        Ok(Self {
            parser,
            max_rows,
            wrap_column,
        })
    }

    pub fn split(&mut self, source: &str) -> Vec<SourceChunk> {
        let Some(tree) = self.parser.parse(source, None) else {
            return vec![self.chunk(source, 0, source.len(), "source")];
        };
        let root = tree.root_node();
        let mut cursor = root.walk();
        let items = root
            .named_children(&mut cursor)
            .filter(|node| is_item(node.kind()))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return vec![self.chunk(source, 0, source.len(), "source")];
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        let mut index = 0;
        while index < items.len() {
            let item = items[index];
            let block_label = match item.kind() {
                "use_declaration" => Some("use block"),
                "mod_item" => Some("mod block"),
                "const_item" => Some("const block"),
                _ => None,
            };
            if let Some(label) = block_label {
                let mut last = index;
                while last + 1 < items.len() && items[last + 1].kind() == item.kind() {
                    last += 1;
                }
                let end = if last + 1 == items.len() {
                    source.len()
                } else {
                    items[last].end_byte()
                };
                self.split_lines(source, start, end, label, &mut chunks);
                start = end;
                index = last + 1;
                continue;
            }

            let end = if index + 1 == items.len() {
                source.len()
            } else {
                item.end_byte()
            };
            self.split_node(
                source,
                item,
                start,
                end,
                &node_label(source, item),
                &mut chunks,
            );
            start = end;
            index += 1;
        }

        for index in 1..chunks.len() {
            let prefix_len = chunks[index]
                .source
                .split_inclusive('\n')
                .take_while(|line| line.ends_with('\n') && line.trim().is_empty())
                .map(str::len)
                .sum::<usize>();
            if prefix_len == 0 {
                continue;
            }

            let (before, after) = chunks.split_at_mut(index);
            let previous = &mut before[index - 1];
            let current = &mut after[0];
            previous
                .source
                .push_str(&current.source.drain(..prefix_len).collect::<String>());
            previous.end_byte += prefix_len;
            current.start_byte += prefix_len;
            (previous.columns, previous.rows) = source_metrics(&previous.source, self.wrap_column);
            (current.columns, current.rows) = source_metrics(&current.source, self.wrap_column);
        }

        chunks
    }

    fn split_node(
        &self,
        source: &str,
        node: Node<'_>,
        start: usize,
        end: usize,
        label: &str,
        output: &mut Vec<SourceChunk>,
    ) {
        if source_metrics(&source[start..end], self.wrap_column).1 <= self.max_rows {
            output.push(self.chunk(source, start, end, label));
            return;
        }

        let children = split_children(node);
        if children.len() < 2 {
            self.split_lines(source, start, end, label, output);
            return;
        }

        let mut part_start = start;
        for (index, child) in children.iter().copied().enumerate() {
            let part_end = if index + 1 == children.len() {
                end
            } else {
                child.end_byte()
            };
            let child_label = node_label(source, child);
            let label = if child_label == child.kind() {
                format!("{label} · {}", index + 1)
            } else {
                child_label
            };
            self.split_node(source, child, part_start, part_end, &label, output);
            part_start = part_end;
        }
    }

    fn split_lines(
        &self,
        source: &str,
        start: usize,
        end: usize,
        label: &str,
        output: &mut Vec<SourceChunk>,
    ) {
        let mut part_start = start;
        let mut cursor = start;
        let mut rows = 0;
        let mut part = 1;
        for line in source[start..end].split_inclusive('\n') {
            let line_rows = visual_width(line.trim_end_matches('\n'))
                .max(1)
                .div_ceil(self.wrap_column);
            if rows > 0 && rows + line_rows > self.max_rows {
                let part_label = format!("{label} · {part}");
                output.push(self.chunk(source, part_start, cursor, &part_label));
                part_start = cursor;
                part += 1;
                rows = 0;
            }
            rows += line_rows;
            cursor += line.len();
        }

        let part_label = (part > 1).then(|| format!("{label} · {part}"));
        output.push(self.chunk(
            source,
            part_start,
            end,
            part_label.as_deref().unwrap_or(label),
        ));
    }

    fn chunk(&self, source: &str, start: usize, end: usize, label: &str) -> SourceChunk {
        let text = source[start..end].to_owned();
        let (columns, rows) = source_metrics(&text, self.wrap_column);
        SourceChunk {
            label: label.to_owned(),
            start_byte: start,
            end_byte: end,
            source: text,
            columns,
            rows,
        }
    }
}

fn split_children(node: Node<'_>) -> Vec<Node<'_>> {
    let container = match node.kind() {
        "impl_item" | "trait_item" | "mod_item" => find_descendant(node, "declaration_list"),
        "function_item" => find_descendant(node, "block"),
        "struct_item" | "union_item" => find_descendant(node, "field_declaration_list"),
        "enum_item" => find_descendant(node, "enum_variant_list"),
        "match_expression" => find_descendant(node, "match_block"),
        _ => None,
    };
    let Some(container) = container else {
        return Vec::new();
    };
    let mut cursor = container.walk();
    container
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .collect()
}

fn find_descendant<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_descendant(child, kind) {
            return Some(found);
        }
    }
    None
}

fn node_label(source: &str, node: Node<'_>) -> String {
    if node.kind() == "use_declaration" {
        return "use".into();
    }
    if let Some(name) = node.child_by_field_name("name") {
        let name = &source[name.start_byte()..name.end_byte()];
        let prefix = match node.kind() {
            "function_item" => "fn",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            "mod_item" => "mod",
            "type_item" => "type",
            "const_item" => "const",
            "static_item" => "static",
            _ => node.kind(),
        };
        return format!("{prefix} {name}");
    }
    if node.kind() == "impl_item" {
        let header = source[node.start_byte()..node.end_byte()]
            .split('{')
            .next()
            .unwrap_or("impl")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        return header.chars().take(64).collect();
    }
    node.kind().replace('_', " ")
}

fn is_item(kind: &str) -> bool {
    matches!(
        kind,
        "const_item"
            | "enum_item"
            | "extern_crate_declaration"
            | "foreign_mod_item"
            | "function_item"
            | "impl_item"
            | "macro_definition"
            | "macro_invocation"
            | "mod_item"
            | "static_item"
            | "struct_item"
            | "trait_item"
            | "type_item"
            | "union_item"
            | "use_declaration"
    )
}

fn source_metrics(source: &str, wrap_column: usize) -> (usize, usize) {
    let widths = source.split('\n').map(visual_width).collect::<Vec<_>>();
    let rows = widths
        .into_iter()
        .map(|width| width.max(1).div_ceil(wrap_column))
        .sum::<usize>()
        .max(1);
    (wrap_column, rows)
}

fn visual_width(line: &str) -> usize {
    line.chars().fold(0, |column, character| {
        if character == '\t' {
            column + (4 - column % 4)
        } else {
            column + 1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::Chunker;

    #[test]
    fn chunks_reconstruct_source_exactly() {
        let source = "use std::fmt;\nuse std::io;\n\nfn one() {}\n\nfn two() {}\n";
        let chunks = Chunker::new(120, 120).unwrap().split(source);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.source.as_str())
                .collect::<String>(),
            source
        );
        assert_eq!(chunks[0].label, "use block");
        assert_eq!(chunks[1].label, "fn one");
        assert!(chunks[0].source.ends_with("\n\n"));
        assert!(!chunks[1].source.starts_with('\n'));
        assert_eq!(chunks[0].end_byte, chunks[1].start_byte);
    }

    #[test]
    fn splits_large_items_without_losing_bytes() {
        let source = "fn large() {\n    one();\n    two();\n    three();\n}\n";
        let chunks = Chunker::new(2, 120).unwrap().split(source);
        assert!(chunks.len() > 1);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.source.as_str())
                .collect::<String>(),
            source
        );
    }

    #[test]
    fn groups_modules_and_uses_fixed_width() {
        let source =
            "mod alpha;\nmod beta;\n\nconst ONE: u8 = 1;\nconst TWO: u8 = 2;\n\nfn main() {}\n";
        let chunks = Chunker::new(120, 120).unwrap().split(source);

        assert_eq!(chunks[0].label, "mod block");
        assert_eq!(chunks[0].columns, 120);
        assert_eq!(chunks[1].label, "const block");
        assert_eq!(chunks[2].label, "fn main");
        assert!(chunks.iter().all(|chunk| chunk.columns == 120));
    }
}
