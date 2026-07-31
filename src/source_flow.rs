use std::ops::Range;

use serde::Serialize;

use crate::syntax::{SourceHighlight, trim_blank_line_range};

pub const GLYPH_WIDTH: f64 = 7.25 * 16.0 / 12.0;
pub const LINE_HEIGHT: f64 = 22.0;
pub const COLUMN_GAP: f64 = 32.0;
pub const MAX_COLUMN_UNITS: usize = 100;

#[derive(Debug, PartialEq, Serialize)]
pub struct SourceFlow {
    pub source: String,
    pub columns: Vec<SourceColumn>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SourceColumn {
    pub rows: Vec<SourceRow>,
    #[serde(skip)]
    pub units: usize,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SourceRow {
    pub runs: Vec<SourceRun>,
    pub highlighted: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SourceRun {
    pub start_byte: usize,
    pub end_byte: usize,
    pub highlighted: bool,
}

pub fn build(
    source: String,
    highlights: &[SourceHighlight],
    visible_ranges: Option<&[Range<usize>]>,
    wrap_column: usize,
) -> SourceFlow {
    let mut highlights = highlights
        .iter()
        .filter(|highlight| {
            visible_ranges.is_none_or(|ranges| {
                ranges.iter().any(|range| {
                    range.start < highlight.element_end_byte
                        && range.end > highlight.lead_start_byte
                })
            })
        })
        .copied()
        .collect::<Vec<_>>();
    highlights.sort_by_key(|highlight| highlight.start_byte);
    let mut rows = visual_rows(&source, &highlights, visible_ranges, wrap_column);
    if rows.is_empty() {
        rows.push(PendingRow {
            range: 0..0,
            row: SourceRow {
                runs: Vec::new(),
                highlighted: false,
            },
        });
    }
    let keep_ends = keep_groups(&source, &rows, &highlights);
    let columns = columns(rows, &keep_ends);
    SourceFlow { source, columns }
}

fn visual_rows(
    source: &str,
    highlights: &[SourceHighlight],
    visible_ranges: Option<&[Range<usize>]>,
    wrap_column: usize,
) -> Vec<PendingRow> {
    let mut rows = Vec::new();
    if let Some(ranges) = visible_ranges {
        for range in ranges {
            let trimmed = trim_blank_line_range(&source[range.clone()]);
            let bounds = range.start + trimmed.start..range.start + trimmed.end;
            visual_rows_in_range(source, bounds, highlights, wrap_column, &mut rows);
        }
        return rows;
    }

    visual_rows_in_range(
        source,
        trim_blank_line_range(source),
        highlights,
        wrap_column,
        &mut rows,
    );
    rows
}

fn visual_rows_in_range(
    source: &str,
    bounds: Range<usize>,
    highlights: &[SourceHighlight],
    wrap_column: usize,
    rows: &mut Vec<PendingRow>,
) {
    let mut line_start = bounds.start;
    while line_start < bounds.end {
        let newline = source[line_start..bounds.end].find('\n');
        let next_line = newline.map_or(bounds.end, |offset| line_start + offset + 1);
        let mut line_end = newline.map_or(bounds.end, |offset| line_start + offset);
        if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
            line_end -= 1;
        }
        wrap_line(source, line_start..line_end, highlights, wrap_column, rows);
        line_start = next_line;
    }
}

fn wrap_line(
    source: &str,
    line: Range<usize>,
    highlights: &[SourceHighlight],
    wrap_column: usize,
    output: &mut Vec<PendingRow>,
) {
    if line.is_empty() {
        output.push(pending_row(line, highlights));
        return;
    }

    let mut row_start = line.start;
    let mut width = 0;
    for (offset, character) in source[line.clone()].char_indices() {
        let byte = line.start + offset;
        let highlighted = is_highlighted(byte, highlights);
        let scale = if highlighted { 2 } else { 1 };
        let mut character_width = if character == '\t' {
            (4 - width % 4) * scale
        } else {
            scale
        };
        if width > 0 && width + character_width > wrap_column {
            output.push(pending_row(row_start..byte, highlights));
            row_start = byte;
            width = 0;
            character_width = if character == '\t' { 4 * scale } else { scale };
        }
        width += character_width;
    }
    output.push(pending_row(row_start..line.end, highlights));
}

fn pending_row(range: Range<usize>, highlights: &[SourceHighlight]) -> PendingRow {
    let mut boundaries = vec![range.start, range.end];
    for highlight in highlights {
        if highlight.start_byte < range.end && highlight.end_byte > range.start {
            boundaries.extend([
                highlight.start_byte.max(range.start),
                highlight.end_byte.min(range.end),
            ]);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let runs = boundaries
        .windows(2)
        .filter(|bounds| bounds[0] < bounds[1])
        .map(|bounds| SourceRun {
            start_byte: bounds[0],
            end_byte: bounds[1],
            highlighted: is_highlighted(bounds[0], highlights),
        })
        .collect::<Vec<_>>();
    let highlighted = runs.iter().any(|run| run.highlighted)
        || highlights.iter().any(|highlight| {
            range.start >= highlight.start_byte && range.start < highlight.end_byte
        });
    PendingRow {
        range,
        row: SourceRow { runs, highlighted },
    }
}

fn keep_groups(
    source: &str,
    rows: &[PendingRow],
    highlights: &[SourceHighlight],
) -> Vec<Option<usize>> {
    let mut keep_ends = vec![None; rows.len()];
    for highlight in highlights {
        let Some(start) = rows.iter().position(|row| {
            row.range.end > highlight.lead_start_byte
                || row.range.start >= highlight.lead_start_byte
        }) else {
            continue;
        };
        let header_end = rows
            .iter()
            .rposition(|row| {
                row.range.start < highlight.end_byte && row.range.end >= highlight.start_byte
            })
            .unwrap_or(start);
        let first_body = rows
            .iter()
            .enumerate()
            .skip(header_end + 1)
            .find(|(_, row)| {
                row.range.start < highlight.element_end_byte
                    && !source[row.range.clone()].trim().is_empty()
            })
            .map_or(header_end, |(index, _)| index);
        keep_ends[start] = Some(first_body);
    }
    keep_ends
}

fn columns(rows: Vec<PendingRow>, keep_ends: &[Option<usize>]) -> Vec<SourceColumn> {
    let units = rows
        .iter()
        .map(|row| if row.row.highlighted { 2 } else { 1 })
        .collect::<Vec<_>>();
    let mut columns = vec![SourceColumn {
        rows: Vec::new(),
        units: 0,
    }];
    for (index, pending) in rows.into_iter().enumerate() {
        let column = columns.last_mut().expect("source column");
        if column.units > 0
            && keep_ends[index].is_some_and(|end| {
                let group_units = units[index..=end].iter().sum::<usize>();
                group_units <= MAX_COLUMN_UNITS && column.units + group_units > MAX_COLUMN_UNITS
            })
        {
            columns.push(SourceColumn {
                rows: Vec::new(),
                units: 0,
            });
        }
        let row_units = units[index];
        let column = columns.last_mut().expect("source column");
        if column.units > 0 && column.units + row_units > MAX_COLUMN_UNITS {
            columns.push(SourceColumn {
                rows: Vec::new(),
                units: 0,
            });
        }
        let column = columns.last_mut().expect("source column");
        column.units += row_units;
        column.rows.push(pending.row);
    }
    columns
}

fn is_highlighted(byte: usize, highlights: &[SourceHighlight]) -> bool {
    highlights
        .iter()
        .any(|highlight| byte >= highlight.start_byte && byte < highlight.end_byte)
}

struct PendingRow {
    range: Range<usize>,
    row: SourceRow,
}

#[cfg(test)]
mod tests {
    use crate::syntax::SourceHighlight;

    use super::{MAX_COLUMN_UNITS, build};

    #[test]
    fn trims_file_edges_but_preserves_internal_blank_rows() {
        let flow = build("\nfn one() {}\n\nfn two() {}\n\n".into(), &[], None, 80);

        assert_eq!(flow.columns.len(), 1);
        assert_eq!(flow.columns[0].rows.len(), 3);
        assert_eq!(flow.columns[0].units, 3);
    }

    #[test]
    fn highlighted_text_uses_double_horizontal_and_vertical_space() {
        let source = "abcdefghij".to_owned();
        let highlight = SourceHighlight {
            lead_start_byte: 0,
            start_byte: 0,
            end_byte: source.len(),
            element_end_byte: source.len(),
        };
        let flow = build(source, &[highlight], None, 10);

        assert_eq!(flow.columns[0].rows.len(), 2);
        assert!(flow.columns[0].rows.iter().all(|row| row.highlighted));
        assert_eq!(flow.columns[0].units, 4);
    }

    #[test]
    fn moves_lead_in_header_and_first_body_line_to_the_next_column() {
        let prefix = (0..98)
            .map(|index| format!("line {index}\n"))
            .collect::<String>();
        let element = "// docs\nfn run() {\n    body();\n}\n";
        let source = format!("{prefix}{element}");
        let lead_start_byte = prefix.len();
        let start_byte = lead_start_byte + "// docs\n".len();
        let end_byte = start_byte + "fn run() {".len();
        let highlight = SourceHighlight {
            lead_start_byte,
            start_byte,
            end_byte,
            element_end_byte: source.len(),
        };
        let flow = build(source, &[highlight], None, 80);

        assert_eq!(flow.columns.len(), 2);
        assert_eq!(flow.columns[0].units, 98);
        assert!(flow.columns[1].units <= MAX_COLUMN_UNITS);
        assert_eq!(flow.columns[1].rows.len(), 4);
        assert!(flow.columns[1].rows[1].highlighted);
    }

    #[test]
    fn renders_only_selected_ranges_against_the_original_source() {
        let source = "fn hidden() {}\npub fn shown() {}\nfn private() {}\n".to_owned();
        let start = source.find("pub").unwrap();
        let end = start + "pub fn shown() {}".len();
        let range = start..end;
        let flow = build(source.clone(), &[], Some(std::slice::from_ref(&range)), 80);
        let runs = &flow.columns[0].rows[0].runs;

        assert_eq!(flow.columns[0].rows.len(), 1);
        assert_eq!(
            &source[runs[0].start_byte..runs[0].end_byte],
            "pub fn shown() {}"
        );
    }
}
