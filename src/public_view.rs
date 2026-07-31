use std::{collections::HashSet, ops::Range};

use crate::project::{NodeKind, ProjectNode};

pub fn build(project: &ProjectNode) -> ProjectNode {
    let mut public_types = HashSet::new();
    collect_public_types(project, &mut public_types);
    let mut public = project.clone();
    public.children = project
        .children
        .iter()
        .filter_map(|child| filter(child, &public_types, false))
        .collect();
    public
}

fn filter(
    node: &ProjectNode,
    public_types: &HashSet<String>,
    inherited_public: bool,
) -> Option<ProjectNode> {
    if matches!(node.kind, NodeKind::Directory | NodeKind::File) {
        let mut filtered = node.clone();
        filtered.children = node
            .children
            .iter()
            .filter_map(|child| filter(child, public_types, false))
            .collect();
        if filtered.children.is_empty() {
            return None;
        }
        if filtered.kind == NodeKind::File
            && let Some(source) = &mut filtered.file_source
        {
            let mut ranges = Vec::new();
            collect_ranges(&filtered.children, &mut ranges);
            source.visible_ranges = Some(normalize_ranges(ranges));
        }
        return Some(filtered);
    }

    let public_trait_impl = node
        .trait_impl_target
        .as_ref()
        .is_some_and(|target| public_types.contains(target));
    let inherited_public = inherited_public || node.public_members || public_trait_impl;
    let mut filtered = node.clone();
    filtered.children = node
        .children
        .iter()
        .filter_map(|child| filter(child, public_types, inherited_public))
        .collect();
    (inherited_public || node.public || !filtered.children.is_empty()).then_some(filtered)
}

fn collect_public_types(node: &ProjectNode, output: &mut HashSet<String>) {
    if let Some(name) = &node.declared_type {
        output.insert(name.clone());
    }
    for child in &node.children {
        collect_public_types(child, output);
    }
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
    use super::build;
    use crate::{
        layout::{self, LayoutConfig, LayoutMode, MapNode},
        project::scan,
        source_flow::SourceFlow,
        syntax::SyntaxTreeBuilder,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn keeps_public_elements_context_and_matching_trait_impls() {
        let directory = temporary_project();
        fs::write(
            directory.join("lib.rs"),
            "mod internal {\n    fn hidden() {}\n    pub(crate) struct Cell;\n    impl Cell {\n        pub fn new() -> Self { Self }\n        fn private() {}\n    }\n    impl Display for Cell {\n        fn fmt() {}\n    }\n}\nfn outside() {}\n",
        )
        .unwrap();
        let project = scan(&directory, 80).unwrap();
        let public = build(&project);
        let file = &public.children[0];
        let module = &file.children[0];

        assert_eq!(module.name, "mod internal");
        assert_eq!(
            module
                .children
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["struct Cell", "impl Cell", "impl Display for Cell"]
        );
        assert_eq!(module.children[1].children.len(), 1);
        assert_eq!(module.children[2].children.len(), 1);
        assert!(file.file_source.as_ref().unwrap().visible_ranges.is_some());

        let packed = layout::pack(
            public,
            LayoutConfig {
                wrap_column: 80,
                mode: LayoutMode::Source,
            },
        );
        let rendered = rendered_source(find_flow(&packed.root).expect("source flow"));
        assert!(rendered.contains("mod internal {"));
        assert!(rendered.contains("pub(crate) struct Cell;"));
        assert!(rendered.contains("pub fn new()"));
        assert!(rendered.contains("impl Display for Cell"));
        assert!(!rendered.contains("fn hidden()"));
        assert!(!rendered.contains("fn private()"));
        assert!(!rendered.contains("fn outside()"));
    }

    #[test]
    fn keeps_all_members_of_public_traits() {
        let mut syntax = SyntaxTreeBuilder::new(80).unwrap();
        let source = "pub trait Service {\n    fn call();\n}\n";
        let nodes = syntax.build(source);

        assert!(nodes[0].public_members);
        assert_eq!(nodes[0].children.len(), 1);
    }

    fn temporary_project() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("source-map-public-view-{}", std::process::id()));
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
