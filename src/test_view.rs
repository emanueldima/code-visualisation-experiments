use std::ops::Range;

use crate::project::{NodeKind, ProjectNode};

pub fn hide(project: &ProjectNode) -> ProjectNode {
    let mut filtered = project.clone();
    filtered.children = project
        .children
        .iter()
        .filter_map(|child| filter(child, false))
        .collect();
    filtered
}

fn filter(node: &ProjectNode, inherited_test: bool) -> Option<ProjectNode> {
    let test = inherited_test || node.test;
    if test {
        return None;
    }

    let mut filtered = node.clone();
    filtered.children = node
        .children
        .iter()
        .filter_map(|child| filter(child, test))
        .collect();
    if matches!(filtered.kind, NodeKind::Directory | NodeKind::File) && filtered.children.is_empty()
    {
        return None;
    }
    if filtered.kind == NodeKind::File
        && let Some(source) = &mut filtered.file_source
    {
        let mut ranges = Vec::new();
        collect_ranges(&filtered.children, &mut ranges);
        source.visible_ranges = Some(normalize_ranges(ranges));
    }
    Some(filtered)
}

fn collect_ranges(nodes: &[ProjectNode], output: &mut Vec<Range<usize>>) {
    for node in nodes {
        output.extend(node.scaffold_ranges.iter().cloned());
        if node.kind == NodeKind::Source
            && let (Some(start), Some(end)) = (node.start_byte, node.end_byte)
        {
            output.push(start..end);
        }
        collect_ranges(&node.children, output);
    }
}

fn normalize_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| range.start);
    let mut normalized: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = normalized.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        normalized.push(range);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::{
        layout::{self, LayoutConfig, LayoutMode, MapNode},
        project::scan,
        source_flow::SourceFlow,
    };

    use super::hide;

    #[test]
    fn hides_test_items_and_test_modules_from_structure_and_source() {
        let directory = temporary_project();
        fs::write(
            directory.join("lib.rs"),
            "pub fn production() {}\n\n#[test]\nfn direct_test() {}\n\n#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n",
        )
        .unwrap();
        let filtered = hide(&scan(&directory, 80).unwrap());
        let file = &filtered.children[0];

        assert_eq!(file.children.len(), 1);
        assert_eq!(file.children[0].name, "fn production");
        let packed = layout::pack(
            filtered,
            LayoutConfig {
                wrap_column: 80,
                mode: LayoutMode::Source,
            },
        );
        let source = rendered_source(find_flow(&packed.root).unwrap());
        assert!(source.contains("fn production"));
        assert!(!source.contains("direct_test"));
        assert!(!source.contains("fn helper"));
    }

    fn temporary_project() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "source-map-test-view-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        if directory.exists() {
            fs::remove_dir_all(&directory).unwrap();
        }
        fs::create_dir(&directory).unwrap();
        directory
    }

    fn find_flow(node: &MapNode) -> Option<&SourceFlow> {
        node.flow
            .as_ref()
            .or_else(|| node.children.iter().find_map(find_flow))
    }

    fn rendered_source(flow: &SourceFlow) -> String {
        flow.columns
            .iter()
            .flat_map(|column| &column.rows)
            .map(|row| {
                row.runs
                    .iter()
                    .map(|run| &flow.source[run.start_byte..run.end_byte])
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
