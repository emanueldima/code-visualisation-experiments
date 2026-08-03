use std::{fs, io, ops::Range, path::Path};

use crate::syntax::{CodeNode, CodeNodeKind, SourceHighlight, SyntaxTreeBuilder};

#[derive(Clone, Debug)]
pub struct FileSource {
    pub text: String,
    pub package: String,
    pub module_path: String,
    pub highlights: Vec<SourceHighlight>,
    pub visible_ranges: Option<Vec<Range<usize>>>,
}

#[derive(Clone, Debug)]
pub struct ProjectNode {
    pub name: String,
    pub subtitle: Option<String>,
    pub path: String,
    pub kind: NodeKind,
    pub test: bool,
    pub public: bool,
    pub public_members: bool,
    pub declared_type: Option<String>,
    pub trait_impl_target: Option<String>,
    pub scaffold_ranges: Vec<Range<usize>>,
    pub source: Option<String>,
    pub file_source: Option<FileSource>,
    pub children: Vec<ProjectNode>,
    pub rows: usize,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    pub preview_key: Option<String>,
    pub details: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Directory,
    File,
    Container,
    Source,
}

pub fn scan(root: &Path, wrap_column: usize) -> io::Result<ProjectNode> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project path is not a directory",
        ));
    }

    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_owned();
    let package = package_name(&root).unwrap_or_else(|| name.clone());
    let mut syntax = SyntaxTreeBuilder::new(wrap_column).map_err(io::Error::other)?;
    let children = scan_directory(&root, &root, &package, &root, &mut syntax)?;

    Ok(ProjectNode {
        name,
        subtitle: None,
        path: String::new(),
        kind: NodeKind::Directory,
        test: false,
        public: false,
        public_members: false,
        declared_type: None,
        trait_impl_target: None,
        scaffold_ranges: Vec::new(),
        source: None,
        file_source: None,
        children,
        rows: 0,
        start_byte: None,
        end_byte: None,
        preview_key: None,
        details: None,
    })
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    inherited_package: &str,
    inherited_package_root: &Path,
    syntax: &mut SyntaxTreeBuilder,
) -> io::Result<Vec<ProjectNode>> {
    let local_package = package_name(directory);
    let package = local_package
        .clone()
        .unwrap_or_else(|| inherited_package.to_owned());
    let package_root = if local_package.is_some() {
        directory
    } else {
        inherited_package_root
    };
    let entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    let mut children = Vec::new();

    for entry in entries {
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            continue;
        };
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if file_type.is_dir() {
            if should_skip_directory(name_text) {
                continue;
            }

            let nested = scan_directory(root, &path, &package, package_root, syntax)?;
            if nested.is_empty() {
                continue;
            }

            children.push(ProjectNode {
                name: name_text.to_owned(),
                subtitle: None,
                path: relative_path(root, &path),
                kind: NodeKind::Directory,
                test: false,
                public: false,
                public_members: false,
                declared_type: None,
                trait_impl_target: None,
                scaffold_ranges: Vec::new(),
                source: None,
                file_source: None,
                children: nested,
                rows: 0,
                start_byte: None,
                end_byte: None,
                preview_key: None,
                details: None,
            });
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }

        let source = fs::read_to_string(&path)?;
        let path_text = relative_path(root, &path);
        let parsed = syntax.build(&source);
        let mut highlights = Vec::new();
        collect_highlights(&parsed, &mut highlights);
        let code = parsed
            .into_iter()
            .map(|node| project_code_node(&path_text, node))
            .collect::<Vec<_>>();
        children.push(ProjectNode {
            name: name_text.to_owned(),
            subtitle: None,
            path: path_text,
            kind: NodeKind::File,
            test: false,
            public: false,
            public_members: false,
            declared_type: None,
            trait_impl_target: None,
            scaffold_ranges: Vec::new(),
            source: None,
            file_source: Some(FileSource {
                text: source,
                package: package.clone(),
                module_path: relative_path(package_root, &path),
                highlights,
                visible_ranges: None,
            }),
            children: code,
            rows: 0,
            start_byte: None,
            end_byte: None,
            preview_key: None,
            details: None,
        });
    }

    children.sort_by(|left, right| {
        matches!(left.kind, NodeKind::Directory)
            .cmp(&matches!(right.kind, NodeKind::Directory))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(children)
}

fn project_code_node(file_path: &str, node: CodeNode) -> ProjectNode {
    ProjectNode {
        name: node.name,
        subtitle: None,
        path: format!("{file_path}#{}-{}", node.start_byte, node.end_byte),
        kind: match node.kind {
            CodeNodeKind::Container => NodeKind::Container,
            CodeNodeKind::Source => NodeKind::Source,
        },
        test: node.test,
        public: node.public,
        public_members: node.public_members,
        declared_type: node.declared_type,
        trait_impl_target: node.trait_impl_target,
        scaffold_ranges: node.scaffold_ranges,
        source: node.source,
        file_source: None,
        children: node
            .children
            .into_iter()
            .map(|child| project_code_node(file_path, child))
            .collect(),
        rows: node.rows,
        start_byte: Some(node.start_byte),
        end_byte: Some(node.end_byte),
        preview_key: None,
        details: None,
    }
}

fn collect_highlights(nodes: &[CodeNode], output: &mut Vec<SourceHighlight>) {
    for node in nodes {
        if let Some(highlight) = node.highlight {
            output.push(highlight);
        }
        collect_highlights(&node.children, output);
    }
}

fn should_skip_directory(name: &str) -> bool {
    name == "target" || name == ".git" || name.starts_with('.')
}

fn package_name(directory: &Path) -> Option<String> {
    let manifest = fs::read_to_string(directory.join("Cargo.toml")).ok()?;
    let package = manifest.split("[package]").nth(1)?.split('\n');
    for line in package {
        let line = line.trim();
        if line.starts_with('[') {
            return None;
        }
        let Some(value) = line
            .strip_prefix("name")
            .and_then(|line| line.trim().strip_prefix('='))
        else {
            continue;
        };
        return Some(value.trim().trim_matches(['\'', '"']).replace('-', "_"));
    }
    None
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
