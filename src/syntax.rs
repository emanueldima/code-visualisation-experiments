use std::ops::Range;

use tree_sitter::{Node, Parser};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeNodeKind {
    Container,
    Source,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceHighlight {
    pub lead_start_byte: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub element_end_byte: usize,
}

#[derive(Debug)]
pub struct CodeNode {
    pub name: String,
    pub kind: CodeNodeKind,
    pub test: bool,
    pub public: bool,
    pub public_members: bool,
    pub declared_type: Option<String>,
    pub trait_impl_target: Option<String>,
    pub scaffold_ranges: Vec<Range<usize>>,
    pub source: Option<String>,
    pub highlight: Option<SourceHighlight>,
    pub children: Vec<CodeNode>,
    pub rows: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

pub struct SyntaxTreeBuilder {
    parser: Parser,
    wrap_column: usize,
}

impl SyntaxTreeBuilder {
    pub fn new(wrap_column: usize) -> Result<Self, tree_sitter::LanguageError> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        Ok(Self {
            parser,
            wrap_column,
        })
    }

    pub fn build(&mut self, source: &str) -> Vec<CodeNode> {
        let Some(tree) = self.parser.parse(source, None) else {
            return vec![self.source_node(source, 0, source.len(), "source")];
        };
        let root = tree.root_node();
        self.partition(source, 0, source.len(), &direct_elements(root, source))
    }

    fn element(
        &self,
        source: &str,
        element: SemanticElement<'_>,
        start: usize,
        end: usize,
    ) -> CodeNode {
        let (children, is_container) = semantic_children(element.node, source);
        let name = node_label(source, element.node);
        let highlight = source_highlight(element);
        let public = is_public(source, element);
        let public_members = public && element.node.kind() == "trait_item";
        let declared_type = public
            .then(|| declared_type(source, element.node))
            .flatten();
        let trait_impl_target = trait_impl_target(source, element.node);
        if !is_container {
            let mut node = self.source_node(source, start, end, &name);
            node.highlight = highlight;
            node.test = element.test;
            node.public = public;
            node.public_members = public_members;
            node.declared_type = declared_type;
            node.trait_impl_target = trait_impl_target;
            return node;
        }

        CodeNode {
            name,
            kind: CodeNodeKind::Container,
            test: element.test,
            public,
            public_members,
            declared_type,
            trait_impl_target,
            scaffold_ranges: scaffold_ranges(element.node, start, end),
            source: None,
            highlight,
            children: self.partition(source, start, end, &children),
            rows: 0,
            start_byte: element.start_byte,
            end_byte: element.node.end_byte(),
        }
    }

    fn partition(
        &self,
        source: &str,
        start: usize,
        end: usize,
        elements: &[SemanticElement<'_>],
    ) -> Vec<CodeNode> {
        if elements.is_empty() {
            return vec![self.source_node(source, start, end, "")];
        }

        let mut children = Vec::new();
        let mut cursor = start;
        let mut index = 0;
        while index < elements.len() {
            let element = elements[index];
            let owned_start = if children.is_empty() {
                cursor
            } else {
                let owned_start = if element.has_leading_comment {
                    source[cursor..element.start_byte]
                        .char_indices()
                        .rfind(|(_, character)| !character.is_whitespace())
                        .map_or(cursor, |(offset, character)| {
                            cursor + offset + character.len_utf8()
                        })
                } else {
                    element.start_byte
                };
                self.extend_node(
                    children.last_mut().expect("previous code block"),
                    source,
                    owned_start,
                );
                owned_start
            };

            if element.node.kind() == "use_declaration" {
                let public = is_public(source, element);
                let mut last = index;
                while last + 1 < elements.len()
                    && elements[last + 1].node.kind() == "use_declaration"
                    && is_public(source, elements[last + 1]) == public
                    && !elements[last + 1].has_leading_comment
                    && source[elements[last].node.end_byte()..elements[last + 1].start_byte]
                        .chars()
                        .all(char::is_whitespace)
                {
                    last += 1;
                }
                let name = if last > index {
                    "use declarations"
                } else {
                    "use declaration"
                };
                let group_end = elements[last].node.end_byte();
                let mut node = self.source_node(source, owned_start, group_end, name);
                node.public = public;
                children.push(node);
                cursor = group_end;
                index = last + 1;
                continue;
            }

            let element_end = element.node.end_byte();
            children.push(self.element(source, element, owned_start, element_end));
            cursor = element_end;
            index += 1;
        }
        self.extend_node(
            children.last_mut().expect("at least one code block"),
            source,
            end,
        );
        children
    }

    fn extend_node(&self, node: &mut CodeNode, source: &str, end: usize) {
        let old_end = node.end_byte;
        if old_end >= end {
            return;
        }

        if let Some(text) = &mut node.source {
            text.push_str(&source[old_end..end]);
            node.rows = source_rows(text, self.wrap_column);
        } else {
            self.extend_node(
                node.children.last_mut().expect("container source block"),
                source,
                end,
            );
            return;
        }
        node.end_byte = end;
    }

    fn source_node(&self, source: &str, start: usize, end: usize, name: &str) -> CodeNode {
        let text = source[start..end].to_owned();
        CodeNode {
            name: if name.is_empty() { "-" } else { name }.to_owned(),
            kind: CodeNodeKind::Source,
            test: false,
            public: false,
            public_members: false,
            declared_type: None,
            trait_impl_target: None,
            scaffold_ranges: Vec::new(),
            rows: source_rows(&text, self.wrap_column),
            source: Some(text),
            highlight: None,
            children: Vec::new(),
            start_byte: start,
            end_byte: end,
        }
    }
}

#[derive(Clone, Copy)]
struct SemanticElement<'tree> {
    node: Node<'tree>,
    start_byte: usize,
    lead_start_byte: usize,
    has_leading_comment: bool,
    test: bool,
}

fn semantic_children<'tree>(
    node: Node<'tree>,
    source: &str,
) -> (Vec<SemanticElement<'tree>>, bool) {
    if node.kind() == "impl_item" {
        let Some(body) = find_descendant(node, "declaration_list") else {
            return (Vec::new(), false);
        };
        let children = direct_elements(body, source);
        let is_container = !children.is_empty();
        return (children, is_container);
    }
    if matches!(node.kind(), "trait_item" | "mod_item" | "foreign_mod_item") {
        let Some(body) = find_descendant(node, "declaration_list") else {
            return (Vec::new(), false);
        };
        return (direct_elements(body, source), true);
    }

    (Vec::new(), false)
}

fn source_highlight(element: SemanticElement<'_>) -> Option<SourceHighlight> {
    let node = element.node;
    let (keyword, end_byte) = match node.kind() {
        "function_item" | "function_signature_item" => {
            ("fn", node.child_by_field_name("name")?.end_byte())
        }
        "impl_item" => ("impl", node.child_by_field_name("type")?.end_byte()),
        "struct_item" => ("struct", node.child_by_field_name("name")?.end_byte()),
        "enum_item" => ("enum", node.child_by_field_name("name")?.end_byte()),
        "trait_item" => ("trait", node.child_by_field_name("name")?.end_byte()),
        "mod_item" => ("mod", node.child_by_field_name("name")?.end_byte()),
        "type_item" => ("type", node.child_by_field_name("name")?.end_byte()),
        "const_item" => ("const", node.child_by_field_name("name")?.end_byte()),
        "static_item" => ("static", node.child_by_field_name("name")?.end_byte()),
        "macro_definition" => ("macro_rules!", node.child_by_field_name("name")?.end_byte()),
        _ => return None,
    };
    let mut cursor = node.walk();
    let start_byte = node
        .children(&mut cursor)
        .find(|child| child.kind() == keyword)?
        .start_byte();
    Some(SourceHighlight {
        lead_start_byte: element.lead_start_byte,
        start_byte,
        end_byte,
        element_end_byte: node.end_byte(),
    })
}

fn direct_elements<'tree>(node: Node<'tree>, source: &str) -> Vec<SemanticElement<'tree>> {
    let mut elements = Vec::new();
    let mut prefix_start = None;
    let mut contiguous_start = None;
    let mut prefix_end_row = None;
    let mut has_leading_comment = false;
    let mut test = false;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "attribute_item" || is_comment(child.kind()) {
            prefix_start.get_or_insert(child.start_byte());
            if prefix_end_row.is_none_or(|row| child.start_position().row > row + 1) {
                contiguous_start = Some(child.start_byte());
            }
            prefix_end_row = Some(child.end_position().row);
            has_leading_comment |= is_comment(child.kind());
            test |= child.kind() == "attribute_item"
                && is_test_attribute(&source[child.start_byte()..child.end_byte()]);
            continue;
        }
        let item = if is_item(child.kind()) {
            Some(child)
        } else if child.kind() == "expression_statement" {
            child
                .named_child(0)
                .filter(|nested| nested.kind() == "macro_invocation")
        } else {
            None
        };
        if let Some(item) = item {
            let prefix_is_contiguous =
                prefix_end_row.is_some_and(|row| item.start_position().row <= row + 1);
            elements.push(SemanticElement {
                node: item,
                start_byte: prefix_start.take().unwrap_or(item.start_byte()),
                lead_start_byte: if prefix_is_contiguous {
                    contiguous_start.unwrap_or(item.start_byte())
                } else {
                    item.start_byte()
                },
                has_leading_comment,
                test,
            });
            contiguous_start = None;
            prefix_end_row = None;
            has_leading_comment = false;
            test = false;
            continue;
        }
        prefix_start = None;
        contiguous_start = None;
        prefix_end_row = None;
        has_leading_comment = false;
        test = false;
    }
    elements
}

fn is_test_attribute(attribute: &str) -> bool {
    let compact = attribute
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let body = compact
        .strip_prefix("#[")
        .and_then(|body| body.strip_suffix(']'))
        .unwrap_or(&compact);
    if body == "cfg(test)" {
        return true;
    }
    let path = body.split('(').next().unwrap_or(body);
    path == "test" || path.ends_with("::test")
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
    if node.kind() == "macro_invocation"
        && let Some(name) = node.child_by_field_name("macro")
    {
        return format!("macro {}", &source[name.start_byte()..name.end_byte()]);
    }
    if let Some(name) = node.child_by_field_name("name") {
        let name = &source[name.start_byte()..name.end_byte()];
        let prefix = match node.kind() {
            "function_item" | "function_signature_item" => "fn",
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
        return header.chars().take(80).collect();
    }
    match node.kind() {
        "closure_expression" => "anonymous fn".into(),
        "async_block" => "async block".into(),
        kind => kind.replace('_', " "),
    }
}

fn is_public(source: &str, element: SemanticElement<'_>) -> bool {
    let mut cursor = element.node.walk();
    if element
        .node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "visibility_modifier")
    {
        return true;
    }

    element.node.kind() == "macro_definition"
        && source[element.start_byte..element.node.start_byte()].contains("macro_export")
}

fn declared_type(source: &str, node: Node<'_>) -> Option<String> {
    if !matches!(
        node.kind(),
        "struct_item" | "enum_item" | "type_item" | "union_item"
    ) {
        return None;
    }

    let name = node.child_by_field_name("name")?;
    Some(source[name.start_byte()..name.end_byte()].to_owned())
}

fn trait_impl_target(source: &str, node: Node<'_>) -> Option<String> {
    if node.kind() != "impl_item" || node.child_by_field_name("trait").is_none() {
        return None;
    }

    let target = node.child_by_field_name("type")?;
    let identifier = last_descendant(target, "type_identifier")?;
    Some(source[identifier.start_byte()..identifier.end_byte()].to_owned())
}

fn scaffold_ranges(node: Node<'_>, start: usize, end: usize) -> Vec<Range<usize>> {
    let Some(body) = find_descendant(node, "declaration_list") else {
        return Vec::new();
    };
    let prefix_end = (body.start_byte() + 1).min(end);
    let suffix_start = body.end_byte().saturating_sub(1).max(start);
    vec![start..prefix_end, suffix_start..end]
}

fn last_descendant<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut found = (node.kind() == kind).then_some(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        found = last_descendant(child, kind).or(found);
    }
    found
}

fn is_item(kind: &str) -> bool {
    matches!(
        kind,
        "const_item"
            | "enum_item"
            | "extern_crate_declaration"
            | "foreign_mod_item"
            | "function_item"
            | "function_signature_item"
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

fn is_comment(kind: &str) -> bool {
    matches!(kind, "line_comment" | "block_comment")
}

fn source_rows(source: &str, wrap_column: usize) -> usize {
    trim_blank_lines(source)
        .split('\n')
        .map(visual_width)
        .map(|width| width.max(1).div_ceil(wrap_column))
        .sum::<usize>()
        .max(1)
}

fn trim_blank_lines(source: &str) -> &str {
    &source[trim_blank_line_range(source)]
}

pub(crate) fn trim_blank_line_range(source: &str) -> Range<usize> {
    let mut offset = 0;
    let mut start = None;
    let mut end = 0;
    for segment in source.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.trim().is_empty() {
            start.get_or_insert(offset);
            end = offset + line.len();
        }
        offset += segment.len();
    }
    start.map_or(0..0, |start| start..end)
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
    use super::{CodeNode, CodeNodeKind, SyntaxTreeBuilder, source_rows, trim_blank_lines};

    #[test]
    fn recursive_leaves_reconstruct_source_exactly() {
        let source = "use std::fmt;\n\nmod inner {\n    impl Thing {\n        fn run(&self) {\n            let callback = || 1;\n            callback();\n        }\n    }\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let mut reconstructed = String::new();
        collect_source(&nodes, &mut reconstructed);

        assert_eq!(reconstructed, source);
        assert!(find_container(&nodes, "mod inner"));
        assert!(find_container(&nodes, "impl Thing"));
        assert!(find_source(&nodes, "fn run"));
        assert!(!find_source(&nodes, "anonymous fn"));
    }

    #[test]
    fn keeps_functions_with_nested_functions_as_single_source_cards() {
        let source =
            "fn outer() {\n    fn inner() {}\n    let callback = || 1;\n    callback();\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let outer = nodes
            .iter()
            .find(|node| node.name == "fn outer")
            .expect("outer function");
        let mut reconstructed = String::new();
        collect_source(&nodes, &mut reconstructed);

        assert_eq!(outer.kind, CodeNodeKind::Source);
        assert_eq!(outer.source.as_deref(), Some(source));
        assert!(outer.children.is_empty());
        assert_eq!(reconstructed, source);
    }

    #[test]
    fn highlights_declaration_headers_but_not_inner_functions() {
        let source = "#[test]\npub async fn outer() {\n    fn inner() {}\n}\n\nimpl<T> Trait for Cell<T> {\n    fn method() {}\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let mut highlights = Vec::new();
        collect_highlights(&nodes, &mut highlights);
        let highlighted = highlights
            .iter()
            .map(|highlight| &source[highlight.start_byte..highlight.end_byte])
            .collect::<Vec<_>>();

        assert_eq!(
            highlighted,
            ["fn outer", "impl<T> Trait for Cell<T>", "fn method"]
        );
        assert_eq!(highlights[0].lead_start_byte, 0);
    }

    #[test]
    fn highlights_macro_definition_header_and_only_contiguous_lead_in() {
        let source = "// detached\n\n#[doc = \"macro\"]\nmacro_rules! make { () => {}; }\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let highlight = nodes[0].highlight.expect("macro highlight");

        assert_eq!(
            &source[highlight.start_byte..highlight.end_byte],
            "macro_rules! make"
        );
        assert_eq!(
            &source[highlight.lead_start_byte..highlight.start_byte],
            "#[doc = \"macro\"]\n"
        );
    }

    #[test]
    fn excludes_visibility_and_generics_from_named_declaration_titles() {
        let source = "pub(crate) struct Cell<T> where T: Copy {\n    value: T,\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let highlight = nodes[0].highlight.expect("struct highlight");

        assert_eq!(
            &source[highlight.start_byte..highlight.end_byte],
            "struct Cell"
        );
    }

    #[test]
    fn keeps_empty_impl_as_a_terminal_source_card() {
        let source = "#[cfg(test)]\nimpl Thing {}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "impl Thing");
        assert_eq!(nodes[0].kind, CodeNodeKind::Source);
        assert_eq!(nodes[0].source.as_deref(), Some(source));
        assert!(nodes[0].children.is_empty());
    }

    #[test]
    fn keeps_container_span_separate_from_owned_source_span() {
        let source = "mod outer {\n    impl Thing {\n        fn run() {}\n    }\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let implementation = &nodes[0].children[0];

        assert_eq!(
            &source[implementation.start_byte..implementation.end_byte],
            "impl Thing {\n        fn run() {}\n    }"
        );
        assert_eq!(first_source(implementation), source);
    }

    #[test]
    fn groups_uses_until_a_comment_breaks_the_run() {
        let source =
            "use std::fmt;\n\n#[cfg(test)]\nuse std::io;\n\n// separate section\nuse std::fs;\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let named = nodes
            .iter()
            .filter(|node| !node.name.is_empty())
            .collect::<Vec<_>>();

        assert_eq!(named.len(), 2);
        assert_eq!(named[0].name, "use declarations");
        assert!(named[0].source.as_deref().unwrap().contains("#[cfg(test)]"));
        assert_eq!(named[1].name, "use declaration");
        assert!(
            named[1]
                .source
                .as_deref()
                .unwrap()
                .contains("// separate section")
        );
    }

    #[test]
    fn identifies_all_restricted_visibilities_and_exported_macros() {
        let source = "pub fn one() {}\npub(crate) struct Two;\npub(super) enum Three {}\npub(in crate) type Four = u8;\n#[macro_export]\nmacro_rules! five { () => {}; }\nfn private() {}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);

        assert!(nodes[..5].iter().all(|node| node.public));
        assert!(!nodes[5].public);
        assert_eq!(nodes[1].declared_type.as_deref(), Some("Two"));
        assert_eq!(nodes[2].declared_type.as_deref(), Some("Three"));
        assert_eq!(nodes[3].declared_type.as_deref(), Some("Four"));
    }

    #[test]
    fn identifies_cfg_and_attribute_tests_without_hiding_mixed_cfg_items() {
        let source = "#[cfg(test)]\nmod tests { fn helper() {} }\n#[tokio::test(flavor = \"current_thread\")]\nasync fn async_test() {}\n#[cfg(any(test, feature = \"extra\"))]\nfn mixed() {}\n";
        let nodes = SyntaxTreeBuilder::new(80).unwrap().build(source);

        assert!(nodes[0].test);
        assert!(nodes[1].test);
        assert!(!nodes[2].test);
    }

    #[test]
    fn separates_public_and_private_use_runs() {
        let source = "use crate::private;\npub(crate) use crate::shared;\npub use crate::external;\nuse crate::other;\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);

        assert_eq!(nodes.len(), 3);
        assert_eq!(
            nodes.iter().map(|node| node.public).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(nodes[1].name, "use declarations");
    }

    #[test]
    fn records_trait_impl_target_but_not_inherent_impl_target() {
        let source = "impl crate::Trait for crate::Cell<u8> {}\nimpl Cell<u8> {}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);

        assert_eq!(nodes[0].trait_impl_target.as_deref(), Some("Cell"));
        assert_eq!(nodes[1].trait_impl_target, None);
    }

    #[test]
    fn attaches_comments_forward_and_trailing_content_backward() {
        let source = "// first\nfn one() {}\n\nlet gap = 1;\n// second\nfn two() {}\n// trailing\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);

        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].source.as_deref().unwrap().starts_with("// first"));
        assert!(nodes[0].source.as_deref().unwrap().contains("let gap = 1;"));
        assert!(
            nodes[1]
                .source
                .as_deref()
                .unwrap()
                .starts_with("\n// second")
        );
        assert!(
            nodes[1]
                .source
                .as_deref()
                .unwrap()
                .ends_with("// trailing\n")
        );
    }

    #[test]
    fn gives_untitled_blocks_a_fallback_title() {
        let nodes = SyntaxTreeBuilder::new(120)
            .unwrap()
            .build("let value = 1;\n");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "-");
    }

    #[test]
    fn labels_macro_invocations_by_name() {
        let nodes = SyntaxTreeBuilder::new(120)
            .unwrap()
            .build("my_crate::declare!(Thing);\n");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "macro my_crate::declare");
    }

    #[test]
    fn attaches_attributes_and_whitespace_to_their_blocks() {
        let source = "#[test]\nfn one() {}\n\n#[cfg(test)]\nmod inner {\n    fn two() {}\n}\n";
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(source);
        let mut reconstructed = String::new();
        collect_source(&nodes, &mut reconstructed);

        assert_eq!(reconstructed, source);
        assert!(nodes[0].source.as_deref().unwrap().starts_with("#[test]"));
        assert!(nodes[0].source.as_deref().unwrap().ends_with("\n\n"));
        assert_eq!(nodes[1].name, "mod inner");
        assert!(first_source(&nodes[1]).starts_with("#[cfg(test)]"));
        assert!(!has_whitespace_only_card(&nodes));
    }

    #[test]
    fn uses_fixed_width_to_measure_wrapped_rows() {
        let source = format!("fn long() {{ {} }}", "x".repeat(130));
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build(&source);
        let function = nodes
            .iter()
            .find(|node| node.name == "fn long")
            .expect("function source");

        assert_eq!(function.kind, CodeNodeKind::Source);
        assert!(function.rows >= 2);
    }

    #[test]
    fn trims_only_boundary_blank_lines_for_layout() {
        let source = "\n \n  fn first() {}\n\n  fn second() {}\n\n";

        assert_eq!(
            trim_blank_lines(source),
            "  fn first() {}\n\n  fn second() {}"
        );
        assert_eq!(source_rows(source, 120), 3);
    }

    #[test]
    fn empty_source_still_has_one_source_card() {
        let nodes = SyntaxTreeBuilder::new(120).unwrap().build("");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, CodeNodeKind::Source);
        assert_eq!(nodes[0].rows, 1);
    }

    fn collect_source(nodes: &[CodeNode], output: &mut String) {
        for node in nodes {
            if let Some(source) = &node.source {
                output.push_str(source);
            } else {
                collect_source(&node.children, output);
            }
        }
    }

    fn collect_highlights(nodes: &[CodeNode], output: &mut Vec<super::SourceHighlight>) {
        for node in nodes {
            if let Some(highlight) = node.highlight {
                output.push(highlight);
            }
            collect_highlights(&node.children, output);
        }
    }

    fn find_container(nodes: &[CodeNode], name: &str) -> bool {
        nodes.iter().any(|node| {
            node.name == name && node.kind == CodeNodeKind::Container
                || find_container(&node.children, name)
        })
    }

    fn find_source(nodes: &[CodeNode], name: &str) -> bool {
        nodes.iter().any(|node| {
            node.name == name && node.kind == CodeNodeKind::Source
                || find_source(&node.children, name)
        })
    }

    fn first_source(node: &CodeNode) -> &str {
        node.source.as_deref().unwrap_or_else(|| {
            node.children
                .iter()
                .find_map(|child| {
                    child
                        .source
                        .as_deref()
                        .or_else(|| Some(first_source(child)))
                })
                .expect("source leaf")
        })
    }

    fn has_whitespace_only_card(nodes: &[CodeNode]) -> bool {
        nodes.iter().any(|node| {
            node.source
                .as_deref()
                .is_some_and(|source| !source.is_empty() && source.trim().is_empty())
                || has_whitespace_only_card(&node.children)
        })
    }
}
