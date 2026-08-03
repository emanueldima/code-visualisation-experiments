use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Parser};

use crate::{
    project::{FileSource, NodeKind, ProjectNode},
    scip_index::{self, SemanticIndex},
};

#[derive(Clone, Debug)]
pub struct CallGraph {
    pub callables: Vec<Callable>,
}

#[derive(Clone, Debug)]
pub struct Callable {
    pub label: String,
    pub full_path: String,
    pub source: String,
    pub rows: usize,
    pub public: bool,
    pub test: bool,
    pub calls: Vec<Call>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub label: String,
    pub target: CallTarget,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CallTarget {
    Resolved(usize),
    Ambiguous(Vec<usize>),
    Dispatch(Vec<usize>),
    External,
    Unresolved,
}

pub fn build(
    project: &ProjectNode,
    semantic: &SemanticIndex,
) -> Result<CallGraph, tree_sitter::LanguageError> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
    let mut drafts = Vec::new();
    let mut public_traits = HashSet::new();
    collect_files(project, &mut |_, source| {
        if let Some(tree) = parser.parse(&source.text, None) {
            collect_public_traits(tree.root_node(), &source.text, &mut public_traits);
        }
    });
    collect_files(project, &mut |file, source| {
        let Some(tree) = parser.parse(&source.text, None) else {
            return;
        };
        let module = module_path(&source.package, &source.module_path);
        collect_items(
            tree.root_node(),
            &source.text,
            file,
            &module,
            None,
            false,
            &public_traits,
            semantic,
            &mut drafts,
        );
    });

    let mut symbols = HashMap::<String, Vec<usize>>::new();
    for (id, draft) in drafts.iter().enumerate() {
        if let Some(symbol) = &draft.symbol {
            symbols.entry(symbol.clone()).or_default().push(id);
        }
    }

    let callables = drafts
        .iter()
        .map(|draft| Callable {
            label: draft.label.clone(),
            full_path: draft.full_path.clone(),
            source: draft.source.clone(),
            rows: draft.rows,
            public: draft.public,
            test: draft.test,
            calls: resolve_calls(draft, &drafts, &symbols, semantic),
        })
        .collect();
    Ok(CallGraph { callables })
}

struct CallableDraft {
    label: String,
    full_path: String,
    symbol: Option<String>,
    source: String,
    rows: usize,
    public: bool,
    test: bool,
    calls: Vec<CallDraft>,
}

struct CallDraft {
    label: String,
    symbol: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn collect_items(
    parent: Node<'_>,
    source: &str,
    file: &ProjectNode,
    module: &[String],
    owner: Option<&str>,
    owner_public: bool,
    public_traits: &HashSet<String>,
    semantic: &SemanticIndex,
    drafts: &mut Vec<CallableDraft>,
) {
    let mut cursor = parent.walk();
    for node in parent.named_children(&mut cursor) {
        match node.kind() {
            "mod_item" if owner.is_none() => {
                let Some(name) = node.child_by_field_name("name") else {
                    continue;
                };
                let Some(body) = direct_child(node, "declaration_list") else {
                    continue;
                };
                let mut nested = module.to_vec();
                nested.push(node_text(source, name).to_owned());
                collect_items(
                    body,
                    source,
                    file,
                    &nested,
                    None,
                    false,
                    public_traits,
                    semantic,
                    drafts,
                );
            }
            "impl_item" if owner.is_none() => {
                let Some(body) = direct_child(node, "declaration_list") else {
                    continue;
                };
                let Some(target) = node.child_by_field_name("type") else {
                    continue;
                };
                let target = final_identifier(node_text(source, target));
                let trait_public = node
                    .child_by_field_name("trait")
                    .map(|trait_node| final_identifier(node_text(source, trait_node)))
                    .is_some_and(|trait_name| public_traits.contains(&trait_name));
                collect_items(
                    body,
                    source,
                    file,
                    module,
                    Some(&target),
                    trait_public,
                    public_traits,
                    semantic,
                    drafts,
                );
            }
            "trait_item" if owner.is_none() => {
                let Some(name) = node.child_by_field_name("name") else {
                    continue;
                };
                let Some(body) = direct_child(node, "declaration_list") else {
                    continue;
                };
                let public = has_visibility(node);
                let name = node_text(source, name);
                collect_items(
                    body,
                    source,
                    file,
                    module,
                    Some(name),
                    public,
                    public_traits,
                    semantic,
                    drafts,
                );
            }
            "function_item" | "function_signature_item" => {
                add_callable(
                    node,
                    source,
                    file,
                    module,
                    owner,
                    owner_public,
                    false,
                    semantic,
                    drafts,
                );
            }
            "macro_definition" if owner.is_none() => {
                add_callable(
                    node, source, file, module, None, false, true, semantic, drafts,
                );
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_callable(
    node: Node<'_>,
    source: &str,
    file: &ProjectNode,
    module: &[String],
    owner: Option<&str>,
    owner_public: bool,
    is_macro: bool,
    semantic: &SemanticIndex,
    drafts: &mut Vec<CallableDraft>,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(source, name_node).to_owned();
    let label = if is_macro {
        format!("macro {name}")
    } else if let Some(owner) = owner {
        format!("{owner}::{name}")
    } else {
        name.clone()
    };
    let mut path = module.to_vec();
    if let Some(owner) = owner {
        path.push(owner.to_owned());
    }
    path.push(name.clone());
    let (owned_source, rows, test) = owned_source(file, node.start_byte(), node.end_byte())
        .unwrap_or_else(|| {
            let source = node_text(source, node).to_owned();
            let rows = source.trim().lines().count().max(1);
            (source, rows, false)
        });
    let mut calls = Vec::new();
    collect_calls(node, source, &file.path, semantic, &mut calls);
    drafts.push(CallableDraft {
        label,
        full_path: path.join("::"),
        symbol: scip_index::symbol_at(
            semantic,
            &file.path,
            name_node.start_byte()..name_node.end_byte(),
        )
        .map(str::to_owned),
        source: owned_source,
        rows,
        public: owner_public || has_visibility(node) || is_macro_public(source, node),
        test,
        calls,
    });
}

fn collect_calls(
    node: Node<'_>,
    source: &str,
    document: &str,
    semantic: &SemanticIndex,
    output: &mut Vec<CallDraft>,
) {
    if node.kind() == "macro_invocation" {
        if let Some(name) = node.child_by_field_name("macro") {
            let name = final_name_node(name);
            output.push(CallDraft {
                label: format!("macro {}", node_text(source, name)),
                symbol: scip_index::symbol_at(
                    semantic,
                    document,
                    name.start_byte()..name.end_byte(),
                )
                .map(str::to_owned),
            });
        }
        return;
    }
    if node.kind() == "call_expression"
        && let Some(function) = node.child_by_field_name("function")
        && let Some(call) = call_draft(function, source, document, semantic)
    {
        output.push(call);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_calls(child, source, document, semantic, output);
    }
}

fn call_draft(
    mut function: Node<'_>,
    source: &str,
    document: &str,
    semantic: &SemanticIndex,
) -> Option<CallDraft> {
    while function.kind() == "generic_function" {
        function = function.child_by_field_name("function")?;
    }
    if function.kind() == "field_expression" {
        function = function.child_by_field_name("field")?;
    } else if function.kind() == "scoped_identifier" {
        function = function.child_by_field_name("name")?;
    } else if function.kind() != "identifier" {
        return None;
    }
    Some(CallDraft {
        label: node_text(source, function).to_owned(),
        symbol: scip_index::symbol_at(
            semantic,
            document,
            function.start_byte()..function.end_byte(),
        )
        .map(str::to_owned),
    })
}

fn resolve_calls(
    callable: &CallableDraft,
    drafts: &[CallableDraft],
    symbols: &HashMap<String, Vec<usize>>,
    semantic: &SemanticIndex,
) -> Vec<Call> {
    let mut seen = HashSet::new();
    callable
        .calls
        .iter()
        .filter_map(|call| {
            let target = call
                .symbol
                .as_deref()
                .map_or(CallTarget::Unresolved, |symbol| {
                    let mut direct = symbols.get(symbol).cloned().unwrap_or_default();
                    let mut implementations = implementation_targets(symbol, symbols, semantic);
                    if !implementations.is_empty() {
                        direct.append(&mut implementations);
                        direct.sort_unstable();
                        direct.dedup();
                        return CallTarget::Dispatch(direct);
                    }
                    match direct.as_slice() {
                        [] if scip_index::is_local(semantic, symbol) => CallTarget::Unresolved,
                        [] => CallTarget::External,
                        [target] => CallTarget::Resolved(*target),
                        _ => CallTarget::Ambiguous(direct),
                    }
                });
            let key = (call.label.clone(), target.clone());
            seen.insert(key).then_some(Call {
                label: call.label.clone(),
                target,
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|mut call| {
            match &mut call.target {
                CallTarget::Ambiguous(candidates) | CallTarget::Dispatch(candidates) => {
                    candidates.sort_by_key(|id| &drafts[*id].full_path);
                }
                _ => {}
            }
            call
        })
        .collect()
}

fn implementation_targets(
    symbol: &str,
    symbols: &HashMap<String, Vec<usize>>,
    semantic: &SemanticIndex,
) -> Vec<usize> {
    let mut pending = semantic
        .implementations
        .get(symbol)
        .cloned()
        .unwrap_or_default();
    let mut visited = HashSet::new();
    let mut targets = Vec::new();
    while let Some(implementation) = pending.pop() {
        if !visited.insert(implementation.clone()) {
            continue;
        }
        if let Some(found) = symbols.get(&implementation) {
            targets.extend(found);
        }
        if let Some(nested) = semantic.implementations.get(&implementation) {
            pending.extend(nested.iter().cloned());
        }
    }
    targets
}

fn final_name_node(mut node: Node<'_>) -> Node<'_> {
    while let Some(name) = node.child_by_field_name("name") {
        node = name;
    }
    node
}

fn module_path(package: &str, file_path: &str) -> Vec<String> {
    let mut parts = file_path.split('/').collect::<Vec<_>>();
    if parts.first() == Some(&"src") {
        parts.remove(0);
    }
    if let Some(last) = parts.last_mut() {
        *last = last.strip_suffix(".rs").unwrap_or(last);
    }
    if parts
        .last()
        .is_some_and(|name| matches!(*name, "lib" | "main" | "mod"))
    {
        parts.pop();
    }
    std::iter::once(package.to_owned())
        .chain(parts.into_iter().map(str::to_owned))
        .collect()
}

fn collect_files<'a>(
    node: &'a ProjectNode,
    action: &mut impl FnMut(&'a ProjectNode, &'a FileSource),
) {
    if let Some(source) = &node.file_source {
        action(node, source);
    }
    for child in &node.children {
        collect_files(child, action);
    }
}

fn collect_public_traits(node: Node<'_>, source: &str, output: &mut HashSet<String>) {
    if node.kind() == "trait_item"
        && has_visibility(node)
        && let Some(name) = node.child_by_field_name("name")
    {
        output.insert(node_text(source, name).to_owned());
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_public_traits(child, source, output);
    }
}

fn owned_source(file: &ProjectNode, start: usize, end: usize) -> Option<(String, usize, bool)> {
    fn find(
        node: &ProjectNode,
        start: usize,
        end: usize,
        inherited_test: bool,
    ) -> Option<(&ProjectNode, bool)> {
        let test = inherited_test || node.test;
        if node.kind == NodeKind::Source
            && node
                .start_byte
                .is_some_and(|node_start| node_start <= start)
            && node.end_byte.is_some_and(|node_end| node_end >= end)
        {
            return Some((node, test));
        }
        node.children
            .iter()
            .find_map(|child| find(child, start, end, test))
    }
    let (node, test) = find(file, start, end, false)?;
    Some((node.source.clone()?, node.rows, test))
}

fn direct_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn has_visibility(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "visibility_modifier")
}

fn is_macro_public(source: &str, node: Node<'_>) -> bool {
    if node.kind() != "macro_definition" {
        return false;
    }
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if previous.kind() != "attribute_item"
            || node.start_position().row > previous.end_position().row + 1
        {
            break;
        }
        if node_text(source, previous).contains("macro_export") {
            return true;
        }
        sibling = previous.prev_named_sibling();
    }
    false
}

fn final_identifier(path: &str) -> String {
    path.rsplit("::")
        .next()
        .unwrap_or(path)
        .split('<')
        .next()
        .unwrap_or(path)
        .trim()
        .to_owned()
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

#[cfg(test)]
mod tests {
    use std::{fs, ops::Range, path::PathBuf};

    use crate::{project::scan, scip_index::fixture};

    use super::{CallTarget, build};

    #[test]
    fn resolves_calls_only_from_scip_symbols() {
        let directory = temporary_project();
        let source = "macro_rules! known { () => {} }\nfn local() {}\nfn missing() {}\nfn run() { local(); external(); missing(); known!(); }\n";
        fs::write(directory.join("lib.rs"), source).unwrap();
        let semantic = fixture(
            &[
                ("lib.rs", nth_range(source, "known", 0), "macro:known", true),
                ("lib.rs", nth_range(source, "local", 0), "fn:local", true),
                (
                    "lib.rs",
                    nth_range(source, "missing", 0),
                    "fn:missing",
                    true,
                ),
                ("lib.rs", nth_range(source, "run", 0), "fn:run", true),
                ("lib.rs", nth_range(source, "local", 1), "fn:local", false),
                (
                    "lib.rs",
                    nth_range(source, "external", 0),
                    "crate:external",
                    false,
                ),
                (
                    "lib.rs",
                    nth_range(source, "known", 1),
                    "macro:known",
                    false,
                ),
            ],
            &[],
        );
        let graph = build(&scan(&directory, 80).unwrap(), &semantic).unwrap();
        let run = graph
            .callables
            .iter()
            .find(|callable| callable.label == "run")
            .unwrap();

        assert_eq!(run.calls.len(), 4);
        assert!(matches!(run.calls[0].target, CallTarget::Resolved(_)));
        assert_eq!(run.calls[1].target, CallTarget::External);
        assert_eq!(run.calls[2].target, CallTarget::Unresolved);
        assert!(matches!(run.calls[3].target, CallTarget::Resolved(_)));
        assert_eq!(run.calls[3].label, "macro known");
    }

    #[test]
    fn expands_scip_implementation_relationships_as_dispatch() {
        let directory = temporary_project();
        let source = "trait Task { fn run(&self); }\nstruct Worker;\nimpl Task for Worker { fn run(&self) {} }\nfn invoke(task: &dyn Task) { task.run(); }\n";
        fs::write(directory.join("lib.rs"), source).unwrap();
        let semantic = fixture(
            &[
                ("lib.rs", nth_range(source, "run", 0), "trait:run", true),
                ("lib.rs", nth_range(source, "run", 1), "impl:run", true),
                ("lib.rs", nth_range(source, "invoke", 0), "fn:invoke", true),
                ("lib.rs", nth_range(source, "run", 2), "trait:run", false),
            ],
            &[("trait:run", "impl:run")],
        );
        let graph = build(&scan(&directory, 80).unwrap(), &semantic).unwrap();
        let invoke = graph
            .callables
            .iter()
            .find(|callable| callable.label == "invoke")
            .unwrap();

        assert!(
            matches!(&invoke.calls[0].target, CallTarget::Dispatch(ids) if ids.len() == 2),
            "{:?}",
            invoke.calls[0].target
        );
    }

    #[test]
    fn resets_module_paths_at_nested_cargo_packages() {
        let directory = temporary_project();
        fs::write(
            directory.join("Cargo.toml"),
            "[workspace]\nmembers = [\"member\"]\n",
        )
        .unwrap();
        fs::create_dir_all(directory.join("member/src")).unwrap();
        fs::write(
            directory.join("member/Cargo.toml"),
            "[package]\nname = \"inner-package\"\n",
        )
        .unwrap();
        fs::write(directory.join("member/src/lib.rs"), "pub fn root() {}\n").unwrap();

        let graph = build(&scan(&directory, 80).unwrap(), &fixture(&[], &[])).unwrap();

        assert_eq!(graph.callables[0].full_path, "inner_package::root");
    }

    #[test]
    fn propagates_test_metadata_to_callables() {
        let directory = temporary_project();
        fs::write(
            directory.join("lib.rs"),
            "fn production() {}\n#[test]\nfn direct_test() {}\n#[cfg(test)]\nmod tests { fn helper() {} }\n",
        )
        .unwrap();

        let graph = build(&scan(&directory, 80).unwrap(), &fixture(&[], &[])).unwrap();

        assert!(
            !graph
                .callables
                .iter()
                .find(|item| item.label == "production")
                .unwrap()
                .test
        );
        assert!(
            graph
                .callables
                .iter()
                .find(|item| item.label == "direct_test")
                .unwrap()
                .test
        );
        assert!(
            graph
                .callables
                .iter()
                .find(|item| item.label == "helper")
                .unwrap()
                .test
        );
    }

    fn temporary_project() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "source-map-call-graph-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        if directory.exists() {
            fs::remove_dir_all(&directory).unwrap();
        }
        fs::create_dir(&directory).unwrap();
        directory
    }

    fn nth_range(source: &str, pattern: &str, occurrence: usize) -> Range<usize> {
        let start = source.match_indices(pattern).nth(occurrence).unwrap().0;
        start..start + pattern.len()
    }
}
