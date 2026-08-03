use std::collections::{HashSet, VecDeque};

use crate::{
    call_graph::{CallGraph, CallTarget},
    project::{NodeKind, ProjectNode},
};

#[derive(Clone, Copy)]
pub struct Options {
    pub public_only: bool,
    pub show_unresolved: bool,
    pub inline_source: bool,
    pub show_tests: bool,
}

pub fn build(graph: &CallGraph, options: Options) -> ProjectNode {
    let mut roots = if options.public_only {
        public_roots(graph, options.show_tests)
    } else {
        let mut incoming = vec![false; graph.callables.len()];
        for callable in &graph.callables {
            if !options.show_tests && callable.test {
                continue;
            }
            for call in &callable.calls {
                match visible_target(graph, &call.target, options.show_tests) {
                    CallTarget::Resolved(target) => incoming[target] = true,
                    CallTarget::Dispatch(targets) => {
                        for target in targets {
                            incoming[target] = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        (0..graph.callables.len())
            .filter(|id| (options.show_tests || !graph.callables[*id].test) && !incoming[*id])
            .collect::<Vec<_>>()
    };
    roots.sort_by_key(|id| &graph.callables[*id].full_path);

    let mut expanded = HashSet::new();
    let mut occurrence = 0;
    let mut children = roots
        .into_iter()
        .map(|id| hierarchy_node(graph, id, options, &mut expanded, &mut occurrence))
        .collect::<Vec<_>>();
    if !options.public_only {
        let mut disconnected = (0..graph.callables.len())
            .filter(|id| {
                !expanded.contains(id) && (options.show_tests || !graph.callables[*id].test)
            })
            .collect::<Vec<_>>();
        disconnected.sort_by_key(|id| &graph.callables[*id].full_path);
        for id in disconnected {
            if expanded.contains(&id) {
                continue;
            }
            children.push(hierarchy_node(
                graph,
                id,
                options,
                &mut expanded,
                &mut occurrence,
            ));
        }
    }

    node("Calls", None, "calls", NodeKind::Directory, None, children)
}

fn hierarchy_node(
    graph: &CallGraph,
    id: usize,
    options: Options,
    expanded: &mut HashSet<usize>,
    occurrence: &mut usize,
) -> ProjectNode {
    let callable = &graph.callables[id];
    if !expanded.insert(id) {
        let mut reference = node(
            &callable.label,
            Some(&callable.full_path),
            &format!("call-reference:{}:{}", callable.full_path, *occurrence),
            NodeKind::Source,
            None,
            Vec::new(),
        );
        *occurrence += 1;
        reference.preview_key = Some(callable.full_path.clone());
        return reference;
    }

    let mut children = Vec::new();
    if options.inline_source {
        let source = trim_blank_lines(&callable.source).to_owned();
        let mut source_node = node(
            "",
            None,
            &callable.full_path,
            NodeKind::Source,
            Some(source),
            Vec::new(),
        );
        source_node.rows = callable.rows;
        children.push(source_node);
    }
    for call in &callable.calls {
        match visible_target(graph, &call.target, options.show_tests) {
            CallTarget::Resolved(target) => {
                children.push(hierarchy_node(graph, target, options, expanded, occurrence))
            }
            CallTarget::Ambiguous(candidates) => {
                let path = format!("ambiguous:{}:{}", callable.full_path, *occurrence);
                *occurrence += 1;
                let mut ambiguous = node(
                    &format!("{} (?)", call.label),
                    Some("ambiguous call"),
                    &path,
                    NodeKind::Source,
                    None,
                    Vec::new(),
                );
                ambiguous.preview_key = Some(path);
                ambiguous.details = Some(format!(
                    "Candidates:\n{}",
                    candidates
                        .iter()
                        .map(|id| graph.callables[*id].full_path.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
                children.push(ambiguous);
            }
            CallTarget::Dispatch(candidates) => {
                let path = format!("dispatch:{}:{}", callable.full_path, *occurrence);
                let implementations = candidates
                    .into_iter()
                    .map(|target| hierarchy_node(graph, target, options, expanded, occurrence))
                    .collect();
                children.push(node(
                    &format!("{} (dispatch)", call.label),
                    Some("dynamic dispatch"),
                    &path,
                    NodeKind::Container,
                    None,
                    implementations,
                ));
            }
            CallTarget::External if options.show_unresolved => children.push(node(
                &format!("{} (external)", call.label),
                Some("external call"),
                &format!("external:{}:{}", callable.full_path, *occurrence),
                NodeKind::Source,
                None,
                Vec::new(),
            )),
            CallTarget::External => {}
            CallTarget::Unresolved if options.show_unresolved => children.push(node(
                &call.label,
                Some("unresolved call"),
                &format!("unresolved:{}:{}", callable.full_path, *occurrence),
                NodeKind::Source,
                None,
                Vec::new(),
            )),
            CallTarget::Unresolved => {}
        }
        *occurrence += 1;
    }
    let mut callable_node = node(
        &callable.label,
        Some(&callable.full_path),
        &callable.full_path,
        NodeKind::Container,
        None,
        children,
    );
    if !options.inline_source {
        callable_node.preview_key = Some(callable.full_path.clone());
    }
    callable_node
}

fn node(
    name: &str,
    subtitle: Option<&str>,
    path: &str,
    kind: NodeKind,
    source: Option<String>,
    children: Vec<ProjectNode>,
) -> ProjectNode {
    ProjectNode {
        name: name.to_owned(),
        subtitle: subtitle.map(str::to_owned),
        path: path.to_owned(),
        kind,
        test: false,
        public: false,
        public_members: false,
        declared_type: None,
        trait_impl_target: None,
        scaffold_ranges: Vec::new(),
        source,
        file_source: None,
        children,
        rows: 0,
        start_byte: None,
        end_byte: None,
        preview_key: None,
        details: None,
    }
}

fn visible_target(graph: &CallGraph, target: &CallTarget, show_tests: bool) -> CallTarget {
    let candidates = match target {
        CallTarget::Resolved(target) => vec![*target],
        CallTarget::Ambiguous(candidates) => candidates.clone(),
        CallTarget::Dispatch(candidates) => {
            let candidates = candidates
                .iter()
                .copied()
                .filter(|target| show_tests || !graph.callables[*target].test)
                .collect::<Vec<_>>();
            return if candidates.is_empty() {
                CallTarget::Unresolved
            } else {
                CallTarget::Dispatch(candidates)
            };
        }
        CallTarget::External => return CallTarget::External,
        CallTarget::Unresolved => return CallTarget::Unresolved,
    }
    .into_iter()
    .filter(|target| show_tests || !graph.callables[*target].test)
    .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => CallTarget::Unresolved,
        [target] => CallTarget::Resolved(*target),
        _ => CallTarget::Ambiguous(candidates),
    }
}

fn public_roots(graph: &CallGraph, show_tests: bool) -> Vec<usize> {
    let mut edges = vec![Vec::new(); graph.callables.len()];
    for (source, callable) in graph.callables.iter().enumerate() {
        if !show_tests && callable.test {
            continue;
        }
        for call in &callable.calls {
            match visible_target(graph, &call.target, show_tests) {
                CallTarget::Resolved(target) => edges[source].push(target),
                CallTarget::Dispatch(targets) => edges[source].extend(targets),
                _ => {}
            }
        }
    }

    let (components, component_count) = strongly_connected_components(&edges);
    let mut representatives = vec![None::<usize>; component_count];
    for (id, callable) in graph.callables.iter().enumerate() {
        if !callable.public || (!show_tests && callable.test) {
            continue;
        }
        let representative = &mut representatives[components[id]];
        if representative
            .is_none_or(|current| callable.full_path < graph.callables[current].full_path)
        {
            *representative = Some(id);
        }
    }

    let mut component_edges = vec![Vec::new(); component_count];
    let mut indegree = vec![0; component_count];
    let mut seen = HashSet::new();
    for (source, targets) in edges.iter().enumerate() {
        for target in targets {
            let edge = (components[source], components[*target]);
            if edge.0 != edge.1 && seen.insert(edge) {
                component_edges[edge.0].push(edge.1);
                indegree[edge.1] += 1;
            }
        }
    }

    let mut queue = indegree
        .iter()
        .enumerate()
        .filter_map(|(component, degree)| (*degree == 0).then_some(component))
        .collect::<VecDeque<_>>();
    let mut public_ancestor = vec![false; component_count];
    while let Some(component) = queue.pop_front() {
        for target in &component_edges[component] {
            public_ancestor[*target] |=
                public_ancestor[component] || representatives[component].is_some();
            indegree[*target] -= 1;
            if indegree[*target] == 0 {
                queue.push_back(*target);
            }
        }
    }

    representatives
        .into_iter()
        .enumerate()
        .filter_map(|(component, representative)| {
            (!public_ancestor[component])
                .then_some(representative)
                .flatten()
        })
        .collect()
}

fn strongly_connected_components(edges: &[Vec<usize>]) -> (Vec<usize>, usize) {
    fn visit(node: usize, edges: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
        if visited[node] {
            return;
        }
        visited[node] = true;
        for target in &edges[node] {
            visit(*target, edges, visited, order);
        }
        order.push(node);
    }

    fn assign(node: usize, edges: &[Vec<usize>], components: &mut [usize], component: usize) {
        if components[node] != usize::MAX {
            return;
        }
        components[node] = component;
        for target in &edges[node] {
            assign(*target, edges, components, component);
        }
    }

    let mut reverse = vec![Vec::new(); edges.len()];
    for (source, targets) in edges.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for node in 0..edges.len() {
        visit(node, edges, &mut visited, &mut order);
    }
    let mut components = vec![usize::MAX; edges.len()];
    let mut component_count = 0;
    for node in order.into_iter().rev() {
        if components[node] != usize::MAX {
            continue;
        }
        assign(node, &reverse, &mut components, component_count);
        component_count += 1;
    }
    (components, component_count)
}

fn trim_blank_lines(source: &str) -> &str {
    let start = source
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(source.len());
    let end = source
        .rfind(|character: char| !character.is_whitespace())
        .map_or(start, |index| {
            index + source[index..].chars().next().unwrap().len_utf8()
        });
    &source[start..end]
}

#[cfg(test)]
mod tests {
    use crate::{
        call_graph::{Call, CallGraph, CallTarget, Callable},
        project::ProjectNode,
    };

    use super::{Options, build};

    #[test]
    fn expands_source_once_and_keeps_references_terminal() {
        let graph = graph();
        let hierarchy = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: true,
                show_tests: true,
            },
        );

        assert_eq!(count_source(&hierarchy, "demo::shared"), 1);
        assert_eq!(count_preview(&hierarchy, "demo::shared"), 2);
        assert_eq!(count_name(&hierarchy, "missing"), 0);
        assert_eq!(count_name(&hierarchy, "choice (?)"), 2);
    }

    #[test]
    fn public_roots_retain_reachable_private_functions() {
        let hierarchy = build(
            &graph(),
            Options {
                public_only: true,
                show_unresolved: true,
                inline_source: true,
                show_tests: true,
            },
        );

        assert_eq!(hierarchy.children.len(), 2);
        assert!(count_name(&hierarchy, "shared") > 0);
        assert_eq!(count_name(&hierarchy, "missing"), 2);
    }

    #[test]
    fn public_roots_exclude_public_functions_reachable_from_other_public_functions() {
        let graph = CallGraph {
            callables: vec![
                callable("entry", true, vec![resolved("helper", 1)]),
                callable("helper", false, vec![resolved("nested_api", 2)]),
                callable("nested_api", true, Vec::new()),
            ],
        };
        let hierarchy = build(
            &graph,
            Options {
                public_only: true,
                show_unresolved: false,
                inline_source: true,
                show_tests: true,
            },
        );

        assert_eq!(hierarchy.children.len(), 1);
        assert_eq!(hierarchy.children[0].name, "entry");
        assert_eq!(count_name(&hierarchy, "nested_api"), 1);
    }

    #[test]
    fn public_recursive_group_uses_alphabetically_first_function_as_root() {
        let graph = CallGraph {
            callables: vec![
                callable("zebra", true, vec![resolved("alpha", 1)]),
                callable("alpha", true, vec![resolved("zebra", 0)]),
            ],
        };
        let hierarchy = build(
            &graph,
            Options {
                public_only: true,
                show_unresolved: false,
                inline_source: true,
                show_tests: true,
            },
        );

        assert_eq!(hierarchy.children.len(), 1);
        assert_eq!(hierarchy.children[0].name, "alpha");
        assert_eq!(count_name(&hierarchy, "zebra"), 1);
    }

    #[test]
    fn represents_a_disconnected_recursive_component_once() {
        let graph = CallGraph {
            callables: vec![
                callable("a", false, vec![resolved("b", 1)]),
                callable("b", false, vec![resolved("a", 0)]),
            ],
        };
        let hierarchy = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: true,
                show_tests: true,
            },
        );

        assert_eq!(hierarchy.children.len(), 1);
        assert_eq!(count_source(&hierarchy, "demo::a"), 1);
        assert_eq!(count_source(&hierarchy, "demo::b"), 1);
    }

    #[test]
    fn compact_hierarchy_uses_hover_source_without_inline_source_nodes() {
        let hierarchy = build(
            &graph(),
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: false,
                show_tests: true,
            },
        );

        assert_eq!(count_sources(&hierarchy), 0);
        assert_eq!(count_preview(&hierarchy, "demo::one"), 1);
        assert_eq!(count_preview(&hierarchy, "demo::shared"), 3);
    }

    #[test]
    fn hidden_test_candidates_no_longer_make_calls_ambiguous() {
        let mut graph = graph();
        graph.callables[4].test = true;
        let hierarchy = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: false,
                show_tests: false,
            },
        );

        assert_eq!(count_name(&hierarchy, "choice (?)"), 0);
        assert_eq!(count_name(&hierarchy, "candidate_b"), 0);
        assert!(count_name(&hierarchy, "candidate_a") > 0);
    }

    #[test]
    fn external_calls_follow_the_unresolved_toggle() {
        let graph = CallGraph {
            callables: vec![callable(
                "entry",
                false,
                vec![Call {
                    label: "dependency".into(),
                    target: CallTarget::External,
                }],
            )],
        };
        let hidden = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: false,
                show_tests: true,
            },
        );
        let shown = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: true,
                inline_source: false,
                show_tests: true,
            },
        );

        assert_eq!(count_name(&hidden, "dependency (external)"), 0);
        assert_eq!(count_name(&shown, "dependency (external)"), 1);
    }

    #[test]
    fn dispatch_targets_are_nested_and_not_rendered_as_roots() {
        let graph = CallGraph {
            callables: vec![
                callable(
                    "entry",
                    false,
                    vec![Call {
                        label: "run".into(),
                        target: CallTarget::Dispatch(vec![1, 2]),
                    }],
                ),
                callable("Trait::run", false, Vec::new()),
                callable("Worker::run", false, Vec::new()),
            ],
        };
        let hierarchy = build(
            &graph,
            Options {
                public_only: false,
                show_unresolved: false,
                inline_source: false,
                show_tests: true,
            },
        );

        assert_eq!(hierarchy.children.len(), 1);
        assert_eq!(hierarchy.children[0].name, "entry");
        assert_eq!(count_name(&hierarchy, "run (dispatch)"), 1);
        assert_eq!(count_name(&hierarchy, "Trait::run"), 1);
        assert_eq!(count_name(&hierarchy, "Worker::run"), 1);
    }

    fn graph() -> CallGraph {
        CallGraph {
            callables: vec![
                callable(
                    "one",
                    true,
                    vec![resolved("shared", 2), ambiguous(), unresolved()],
                ),
                callable(
                    "two",
                    true,
                    vec![resolved("shared", 2), ambiguous(), unresolved()],
                ),
                callable("shared", false, vec![resolved("shared", 2)]),
                callable("candidate_a", false, Vec::new()),
                callable("candidate_b", false, Vec::new()),
            ],
        }
    }

    fn callable(name: &str, public: bool, calls: Vec<Call>) -> Callable {
        Callable {
            label: name.into(),
            full_path: format!("demo::{name}"),
            source: format!("fn {name}() {{}}"),
            rows: 1,
            public,
            test: false,
            calls,
        }
    }

    fn resolved(label: &str, id: usize) -> Call {
        Call {
            label: label.into(),
            target: CallTarget::Resolved(id),
        }
    }

    fn ambiguous() -> Call {
        Call {
            label: "choice".into(),
            target: CallTarget::Ambiguous(vec![3, 4]),
        }
    }

    fn unresolved() -> Call {
        Call {
            label: "missing".into(),
            target: CallTarget::Unresolved,
        }
    }

    fn count_name(node: &ProjectNode, name: &str) -> usize {
        usize::from(node.name == name)
            + node
                .children
                .iter()
                .map(|child| count_name(child, name))
                .sum::<usize>()
    }

    fn count_source(node: &ProjectNode, path: &str) -> usize {
        usize::from(node.path == path && node.source.is_some())
            + node
                .children
                .iter()
                .map(|child| count_source(child, path))
                .sum::<usize>()
    }

    fn count_sources(node: &ProjectNode) -> usize {
        usize::from(node.source.is_some()) + node.children.iter().map(count_sources).sum::<usize>()
    }

    fn count_preview(node: &ProjectNode, key: &str) -> usize {
        usize::from(node.preview_key.as_deref() == Some(key))
            + node
                .children
                .iter()
                .map(|child| count_preview(child, key))
                .sum::<usize>()
    }
}
