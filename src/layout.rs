use serde::Serialize;
use std::ops::Range;

use crate::project::{NodeKind, ProjectNode};

#[derive(Serialize)]
pub struct PackedMap {
    pub root: MapNode,
    pub aspect_ratio: f64,
    pub world_width: f64,
    pub world_height: f64,
}

#[derive(Serialize)]
pub struct MapNode {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub rect: Rect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub children: Vec<MapNode>,
    pub columns: usize,
    pub rows: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_byte: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_byte: Option<usize>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn pack(root: ProjectNode, target_aspect: f64) -> PackedMap {
    let packed = pack_node(root, target_aspect);
    PackedMap {
        root: packed.map,
        aspect_ratio: packed.width / packed.height,
        world_width: packed.width,
        world_height: packed.height,
    }
}

impl Rect {
    const UNIT: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
}

struct PackedNode {
    map: MapNode,
    width: f64,
    height: f64,
}

fn pack_node(node: ProjectNode, target_aspect: f64) -> PackedNode {
    let ProjectNode {
        name,
        path,
        kind,
        source,
        children,
        columns,
        rows,
        start_byte,
        end_byte,
        ..
    } = node;
    if matches!(kind, NodeKind::Chunk) {
        return PackedNode {
            map: MapNode {
                name,
                path,
                kind: "chunk",
                rect: Rect::UNIT,
                source,
                children: Vec::new(),
                columns,
                rows,
                start_byte,
                end_byte,
            },
            width: columns.max(1) as f64 * GLYPH_WIDTH + CHUNK_PADDING * 2.0,
            height: rows.max(1) as f64 * LINE_HEIGHT + CHUNK_PADDING * 2.0,
        };
    }

    let mut packed_children = children
        .into_iter()
        .map(|child| pack_node(child, target_aspect))
        .collect::<Vec<_>>();
    let gutter = if matches!(kind, NodeKind::File) {
        FILE_GUTTER
    } else {
        DIRECTORY_GUTTER
    };
    let (width, height, rectangles) = if matches!(kind, NodeKind::File) {
        pack_file(&packed_children, target_aspect, gutter)
    } else {
        pack_shelves(&packed_children, target_aspect, gutter)
    };
    for (child, rect) in packed_children.iter_mut().zip(rectangles) {
        child.map.rect = rect;
    }

    PackedNode {
        map: MapNode {
            name,
            path,
            kind: if matches!(kind, NodeKind::File) {
                "file"
            } else {
                "directory"
            },
            rect: Rect::UNIT,
            source: None,
            children: packed_children.into_iter().map(|child| child.map).collect(),
            columns,
            rows,
            start_byte,
            end_byte,
        },
        width,
        height,
    }
}

fn pack_file(nodes: &[PackedNode], target_aspect: f64, gutter: f64) -> (f64, f64, Vec<Rect>) {
    if nodes.is_empty() {
        return (1.0, 1.0, Vec::new());
    }

    let mut best = None;
    for column_count in 1..=nodes.len() {
        let columns = partition_columns(nodes, column_count, gutter);
        let widths = columns
            .iter()
            .map(|column| {
                nodes[column.clone()]
                    .iter()
                    .map(|node| node.width)
                    .fold(0.0_f64, f64::max)
            })
            .collect::<Vec<_>>();
        let content_height = columns
            .iter()
            .map(|column| {
                nodes[column.clone()]
                    .iter()
                    .map(|node| node.height)
                    .sum::<f64>()
                    + gutter * column.len().saturating_sub(1) as f64
            })
            .fold(0.0_f64, f64::max);
        let width = widths.iter().sum::<f64>() + gutter * (column_count + 1) as f64;
        let height = content_height + gutter * 2.0 + FILE_HEADER_HEIGHT;
        let score = ((width / height) / target_aspect).ln().abs();
        if best
            .as_ref()
            .is_some_and(|(best_score, ..): &(f64, _, _, _, _)| *best_score <= score)
        {
            continue;
        }
        best = Some((score, width, height, columns, widths));
    }

    let (_, width, height, columns, widths) = best.expect("nodes are not empty");
    let mut rectangles = vec![Rect::UNIT; nodes.len()];
    let mut x = gutter;
    for (column, column_width) in columns.into_iter().zip(widths) {
        let mut y = gutter + FILE_HEADER_HEIGHT;
        for index in column {
            rectangles[index] = Rect {
                x: x / width,
                y: y / height,
                width: nodes[index].width / width,
                height: nodes[index].height / height,
            };
            y += nodes[index].height + gutter;
        }
        x += column_width + gutter;
    }

    (width, height, rectangles)
}

fn partition_columns(nodes: &[PackedNode], column_count: usize, gutter: f64) -> Vec<Range<usize>> {
    let mut suffix_heights = vec![0.0; nodes.len() + 1];
    for index in (0..nodes.len()).rev() {
        suffix_heights[index] = suffix_heights[index + 1] + nodes[index].height + gutter;
    }

    let mut columns = Vec::with_capacity(column_count);
    let mut start = 0;
    for column_index in 0..column_count {
        let remaining_columns = column_count - column_index;
        if remaining_columns == 1 {
            columns.push(start..nodes.len());
            break;
        }

        let target_height = suffix_heights[start] / remaining_columns as f64;
        let last_allowed = nodes.len() - (remaining_columns - 1);
        let mut end = start + 1;
        let mut height = nodes[start].height + gutter;
        while end < last_allowed {
            let candidate = height + nodes[end].height + gutter;
            if (height - target_height).abs() <= (candidate - target_height).abs() {
                break;
            }
            height = candidate;
            end += 1;
        }
        columns.push(start..end);
        start = end;
    }
    columns
}

fn pack_shelves(nodes: &[PackedNode], target_aspect: f64, gutter: f64) -> (f64, f64, Vec<Rect>) {
    if nodes.is_empty() {
        return (1.0, 1.0, Vec::new());
    }

    let total_area = nodes
        .iter()
        .map(|node| node.width * node.height)
        .sum::<f64>();
    let target_width = (total_area * target_aspect).sqrt();
    let mut rows = Vec::new();
    let mut start = 0;
    while start < nodes.len() {
        let mut end = start + 1;
        let mut width = nodes[start].width;
        while end < nodes.len() {
            let candidate = width + gutter + nodes[end].width;
            if (width - target_width).abs() <= (candidate - target_width).abs() {
                break;
            }
            width = candidate;
            end += 1;
        }
        rows.push(start..end);
        start = end;
    }

    let width = rows
        .iter()
        .map(|row| {
            nodes[row.clone()]
                .iter()
                .map(|node| node.width)
                .sum::<f64>()
                + gutter * row.len().saturating_sub(1) as f64
        })
        .fold(0.0_f64, f64::max)
        + gutter * 2.0;
    let height = rows
        .iter()
        .map(|row| {
            nodes[row.clone()]
                .iter()
                .map(|node| node.height)
                .fold(0.0_f64, f64::max)
        })
        .sum::<f64>()
        + gutter * (rows.len() + 1) as f64;
    let mut rectangles = vec![Rect::UNIT; nodes.len()];
    let mut y = gutter;
    for row in rows {
        let row_height = nodes[row.clone()]
            .iter()
            .map(|node| node.height)
            .fold(0.0_f64, f64::max);
        let mut x = gutter;
        for index in row {
            rectangles[index] = Rect {
                x: x / width,
                y: y / height,
                width: nodes[index].width / width,
                height: nodes[index].height / height,
            };
            x += nodes[index].width + gutter;
        }
        y += row_height + gutter;
    }

    (width, height, rectangles)
}

const GLYPH_WIDTH: f64 = 7.2;
const LINE_HEIGHT: f64 = 16.2;
const CHUNK_PADDING: f64 = 3.0;
const FILE_GUTTER: f64 = 4.0;
const FILE_HEADER_HEIGHT: f64 = 18.0;
const DIRECTORY_GUTTER: f64 = 8.0;

#[cfg(test)]
mod tests {
    use crate::project::{NodeKind, ProjectNode};

    use super::pack;

    #[test]
    fn preserves_natural_file_ratios() {
        let map = pack(
            directory(vec![chunk("a", 40, 100), chunk("b", 40, 100)]),
            1.6,
        );
        let children = &map.root.children;

        assert_eq!(children[0].rect.x, 8.0 / 612.0);
        assert_eq!(children[0].rect.width, 294.0 / 612.0);
        assert_eq!(children[1].rect.x, 310.0 / 612.0);
        assert_eq!(map.world_width, 612.0);
        assert_eq!(map.world_height, 1642.0);
        assert!((map.aspect_ratio - 612.0 / 1642.0).abs() < f64::EPSILON);
    }

    #[test]
    fn packs_file_chunks_down_contiguous_columns() {
        let chunks = (0..6)
            .map(|index| chunk(&index.to_string(), 120, 100))
            .collect();
        let map = pack(file(chunks), 1.6);
        let children = &map.root.children;

        assert!(
            children
                .windows(2)
                .all(|pair| pair[0].rect.x <= pair[1].rect.x)
        );
        let first_chunk_y = children[0].rect.y * map.world_height;
        assert!(
            (first_chunk_y - (super::FILE_GUTTER + super::FILE_HEADER_HEIGHT)).abs() < f64::EPSILON
        );
        assert_eq!(children[4].rect.x, children[5].rect.x);
        assert!(children[4].rect.y < children[5].rect.y);
    }

    fn directory(children: Vec<ProjectNode>) -> ProjectNode {
        ProjectNode {
            name: "root".into(),
            path: String::new(),
            kind: NodeKind::Directory,
            source: None,
            children,
            columns: 0,
            rows: 0,
            weight: 1.0,
            start_byte: None,
            end_byte: None,
        }
    }

    fn file(children: Vec<ProjectNode>) -> ProjectNode {
        ProjectNode {
            name: "main.rs".into(),
            path: "main.rs".into(),
            kind: NodeKind::File,
            source: None,
            children,
            columns: 0,
            rows: 0,
            weight: 1.0,
            start_byte: None,
            end_byte: None,
        }
    }

    fn chunk(name: &str, columns: usize, rows: usize) -> ProjectNode {
        ProjectNode {
            name: name.into(),
            path: name.into(),
            kind: NodeKind::Chunk,
            source: Some(String::new()),
            children: Vec::new(),
            columns,
            rows,
            weight: (columns * rows) as f64,
            start_byte: Some(0),
            end_byte: Some(0),
        }
    }
}
