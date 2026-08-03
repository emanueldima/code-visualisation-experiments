use std::{
    collections::{HashMap, HashSet},
    fs, io,
    ops::Range,
    path::Path,
};

use protobuf::Message;
use scip::types::{self, PositionEncoding, occurrence};

use crate::project::{FileSource, ProjectNode};

#[derive(Debug)]
pub struct SemanticIndex {
    occurrences: HashMap<String, Vec<SemanticOccurrence>>,
    local_symbols: HashSet<String>,
    pub implementations: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
struct SemanticOccurrence {
    range: Range<usize>,
    symbol: String,
}

pub fn load(
    path: &Path,
    project: &ProjectNode,
) -> Result<SemanticIndex, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let index = types::Index::parse_from_bytes(&bytes)?;
    Ok(build(index, project))
}

pub fn symbol_at<'a>(
    index: &'a SemanticIndex,
    document: &str,
    range: Range<usize>,
) -> Option<&'a str> {
    index
        .occurrences
        .get(&normalize_path(document))?
        .iter()
        .filter(|occurrence| {
            occurrence.range.start < range.end && range.start < occurrence.range.end
        })
        .min_by_key(|occurrence| {
            (
                occurrence.range != range,
                occurrence.range.start.abs_diff(range.start)
                    + occurrence.range.end.abs_diff(range.end),
            )
        })
        .map(|occurrence| occurrence.symbol.as_str())
}

pub fn is_local(index: &SemanticIndex, symbol: &str) -> bool {
    index.local_symbols.contains(symbol)
}

pub fn is_stale(root: &Path, project: &ProjectNode, index_path: &Path) -> io::Result<bool> {
    let indexed = fs::metadata(index_path)?.modified()?;
    let mut stale = false;
    visit_files(project, &mut |file, _| {
        if !stale {
            stale = fs::metadata(root.join(&file.path))
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified > indexed);
        }
    });
    Ok(stale)
}

pub fn build_command(root: &Path, index_path: &Path) -> String {
    format!(
        "rust-analyzer scip {} --output {}",
        shell_quote(root),
        shell_quote(index_path)
    )
}

#[cfg(test)]
pub fn fixture(
    occurrences: &[(&str, Range<usize>, &str, bool)],
    implementation_pairs: &[(&str, &str)],
) -> SemanticIndex {
    let mut documents = HashMap::<String, Vec<SemanticOccurrence>>::new();
    let mut local_symbols = HashSet::new();
    for (document, range, symbol, definition) in occurrences {
        documents
            .entry(normalize_path(document))
            .or_default()
            .push(SemanticOccurrence {
                range: range.clone(),
                symbol: (*symbol).to_owned(),
            });
        if *definition {
            local_symbols.insert((*symbol).to_owned());
        }
    }
    let mut implementations = HashMap::<String, Vec<String>>::new();
    for (abstraction, implementation) in implementation_pairs {
        implementations
            .entry((*abstraction).to_owned())
            .or_default()
            .push((*implementation).to_owned());
    }
    SemanticIndex {
        occurrences: documents,
        local_symbols,
        implementations,
    }
}

fn build(index: types::Index, project: &ProjectNode) -> SemanticIndex {
    let mut sources = HashMap::new();
    visit_files(project, &mut |file, source| {
        sources.insert(normalize_path(&file.path), source.text.as_str());
    });
    let mut occurrences = HashMap::new();
    let mut local_symbols = HashSet::new();
    let mut implementations = HashMap::<String, Vec<String>>::new();

    for document in index.documents {
        let path = normalize_path(&document.relative_path);
        let Some(source) = sources.get(&path) else {
            continue;
        };
        let encoding = document.position_encoding.enum_value_or_default();
        let mut semantic_occurrences = Vec::new();
        for occurrence in document.occurrences {
            if occurrence.symbol.is_empty() {
                continue;
            }
            let Some(range) =
                occurrence_range(&occurrence).and_then(|range| byte_range(source, range, encoding))
            else {
                continue;
            };
            if occurrence.symbol_roles & 1 != 0 {
                local_symbols.insert(occurrence.symbol.clone());
            }
            semantic_occurrences.push(SemanticOccurrence {
                range,
                symbol: occurrence.symbol,
            });
        }
        semantic_occurrences.sort_by_key(|occurrence| occurrence.range.start);
        occurrences.insert(path, semantic_occurrences);

        for symbol in document.symbols {
            local_symbols.insert(symbol.symbol.clone());
            for relationship in symbol.relationships {
                if relationship.is_implementation {
                    implementations
                        .entry(relationship.symbol)
                        .or_default()
                        .push(symbol.symbol.clone());
                }
            }
        }
    }
    for symbols in implementations.values_mut() {
        symbols.sort();
        symbols.dedup();
    }
    SemanticIndex {
        occurrences,
        local_symbols,
        implementations,
    }
}

fn occurrence_range(occurrence: &types::Occurrence) -> Option<(usize, usize, usize, usize)> {
    let typed = match &occurrence.typed_range {
        Some(occurrence::Typed_range::SingleLineRange(range)) => Some((
            usize::try_from(range.line).ok()?,
            usize::try_from(range.start_character).ok()?,
            usize::try_from(range.line).ok()?,
            usize::try_from(range.end_character).ok()?,
        )),
        Some(occurrence::Typed_range::MultiLineRange(range)) => Some((
            usize::try_from(range.start_line).ok()?,
            usize::try_from(range.start_character).ok()?,
            usize::try_from(range.end_line).ok()?,
            usize::try_from(range.end_character).ok()?,
        )),
        _ => None,
    };
    if typed.is_some() {
        return typed;
    }
    match occurrence.range.as_slice() {
        [line, start, end] => Some((
            usize::try_from(*line).ok()?,
            usize::try_from(*start).ok()?,
            usize::try_from(*line).ok()?,
            usize::try_from(*end).ok()?,
        )),
        [start_line, start, end_line, end] => Some((
            usize::try_from(*start_line).ok()?,
            usize::try_from(*start).ok()?,
            usize::try_from(*end_line).ok()?,
            usize::try_from(*end).ok()?,
        )),
        _ => None,
    }
}

fn byte_range(
    source: &str,
    (start_line, start, end_line, end): (usize, usize, usize, usize),
    encoding: PositionEncoding,
) -> Option<Range<usize>> {
    let line_starts = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect::<Vec<_>>();
    Some(
        position(source, &line_starts, start_line, start, encoding)?
            ..position(source, &line_starts, end_line, end, encoding)?,
    )
}

fn position(
    source: &str,
    line_starts: &[usize],
    line: usize,
    character: usize,
    encoding: PositionEncoding,
) -> Option<usize> {
    let start = *line_starts.get(line)?;
    let end = line_starts
        .get(line + 1)
        .map_or(source.len(), |next| next - 1);
    let text = &source[start..end];
    let offset = match encoding {
        PositionEncoding::UTF16CodeUnitOffsetFromLineStart => {
            encoded_offset(text, character, char::len_utf16)?
        }
        PositionEncoding::UTF32CodeUnitOffsetFromLineStart => {
            encoded_offset(text, character, |_| 1)?
        }
        PositionEncoding::UnspecifiedPositionEncoding
        | PositionEncoding::UTF8CodeUnitOffsetFromLineStart => {
            (character <= text.len() && text.is_char_boundary(character)).then_some(character)?
        }
    };
    Some(start + offset)
}

fn encoded_offset(text: &str, target: usize, width: impl Fn(char) -> usize) -> Option<usize> {
    let mut units = 0;
    for (offset, character) in text.char_indices() {
        if units == target {
            return Some(offset);
        }
        units += width(character);
        if units > target {
            return None;
        }
    }
    (units == target).then_some(text.len())
}

fn visit_files<'a>(
    node: &'a ProjectNode,
    action: &mut impl FnMut(&'a ProjectNode, &'a FileSource),
) {
    if let Some(source) = &node.file_source {
        action(node, source);
    }
    for child in &node.children {
        visit_files(child, action);
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use std::{fs, thread, time::Duration};

    use protobuf::EnumOrUnknown;
    use scip::types::{
        Document, Index, Occurrence, PositionEncoding, Relationship, SymbolInformation,
    };

    use crate::project::scan;

    use super::{build_command, is_stale, load, symbol_at};

    #[test]
    fn loads_utf16_occurrences_and_implementation_relationships() {
        let directory = temporary_project("load");
        fs::write(directory.join("lib.rs"), "fn café() {}\n").unwrap();
        let project = scan(&directory, 80).unwrap();
        let mut index = Index::new();
        let mut document = Document::new();
        document.relative_path = "lib.rs".into();
        document.position_encoding =
            EnumOrUnknown::new(PositionEncoding::UTF16CodeUnitOffsetFromLineStart);
        document.occurrences.push(Occurrence {
            range: vec![0, 3, 7],
            symbol: "demo café().".into(),
            symbol_roles: 1,
            ..Occurrence::new()
        });
        document.symbols.push(SymbolInformation {
            symbol: "demo café().".into(),
            relationships: vec![Relationship {
                symbol: "demo Trait#café().".into(),
                is_implementation: true,
                ..Relationship::new()
            }],
            ..SymbolInformation::new()
        });
        index.documents.push(document);
        let path = directory.join("index.scip");
        scip::write_message_to_file(&path, index).unwrap();

        let semantic = load(&path, &project).unwrap();

        assert_eq!(symbol_at(&semantic, "lib.rs", 3..8), Some("demo café()."));
        assert_eq!(
            semantic.implementations["demo Trait#café()."],
            ["demo café()."]
        );
    }

    #[test]
    fn reports_stale_sources_and_quotes_build_command() {
        let directory = temporary_project("stale ' path");
        fs::write(directory.join("lib.rs"), "fn old() {}\n").unwrap();
        let project = scan(&directory, 80).unwrap();
        let index = directory.join("index.scip");
        fs::write(&index, []).unwrap();
        thread::sleep(Duration::from_millis(10));
        fs::write(directory.join("lib.rs"), "fn new() {}\n").unwrap();

        assert!(is_stale(&directory, &project, &index).unwrap());
        assert!(build_command(&directory, &index).contains("'\"'\"'"));
    }

    fn temporary_project(suffix: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("source-map-scip-{}-{suffix}", std::process::id()));
        if directory.exists() {
            fs::remove_dir_all(&directory).unwrap();
        }
        fs::create_dir(&directory).unwrap();
        directory
    }
}
