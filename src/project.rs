use std::{fs, io, path::Path};

use crate::syntax::Chunker;

#[derive(Clone, Debug)]
pub struct ProjectNode {
    pub name: String,
    pub path: String,
    pub kind: NodeKind,
    pub source: Option<String>,
    pub children: Vec<ProjectNode>,
    pub columns: usize,
    pub rows: usize,
    pub weight: f64,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub enum NodeKind {
    Directory,
    File,
    Chunk,
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
    let mut chunker = Chunker::new(120, wrap_column).map_err(io::Error::other)?;
    let children = scan_directory(&root, &root, &mut chunker)?;
    let weight = children
        .iter()
        .map(|child| child.weight)
        .sum::<f64>()
        .max(1.0);

    Ok(ProjectNode {
        name,
        path: String::new(),
        kind: NodeKind::Directory,
        source: None,
        children,
        columns: 0,
        rows: 0,
        weight,
        start_byte: None,
        end_byte: None,
    })
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    chunker: &mut Chunker,
) -> io::Result<Vec<ProjectNode>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
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

            let nested = scan_directory(root, &path, chunker)?;
            if nested.is_empty() {
                continue;
            }

            let weight = nested
                .iter()
                .map(|child| child.weight)
                .sum::<f64>()
                .max(1.0);
            children.push(ProjectNode {
                name: name_text.to_owned(),
                path: relative_path(root, &path),
                kind: NodeKind::Directory,
                source: None,
                children: nested,
                columns: 0,
                rows: 0,
                weight,
                start_byte: None,
                end_byte: None,
            });
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }

        let source = fs::read_to_string(&path)?;
        let path_text = relative_path(root, &path);
        let chunks = chunker
            .split(&source)
            .into_iter()
            .map(|chunk| ProjectNode {
                name: chunk.label,
                path: format!("{path_text}#{}-{}", chunk.start_byte, chunk.end_byte),
                kind: NodeKind::Chunk,
                source: Some(chunk.source),
                children: Vec::new(),
                columns: chunk.columns,
                rows: chunk.rows,
                weight: (chunk.columns * chunk.rows).max(1) as f64,
                start_byte: Some(chunk.start_byte),
                end_byte: Some(chunk.end_byte),
            })
            .collect::<Vec<_>>();
        let weight = chunks
            .iter()
            .map(|chunk| chunk.weight)
            .sum::<f64>()
            .max(1.0);
        children.push(ProjectNode {
            name: name_text.to_owned(),
            path: path_text,
            kind: NodeKind::File,
            source: None,
            children: chunks,
            columns: 0,
            rows: 0,
            weight,
            start_byte: None,
            end_byte: None,
        });
    }

    Ok(children)
}

fn should_skip_directory(name: &str) -> bool {
    name == "target" || name == ".git" || name.starts_with('.')
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
