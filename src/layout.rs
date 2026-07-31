use serde::Serialize;

use crate::{
    project::{FileSource, NodeKind, ProjectNode},
    source_flow::{self, SourceFlow},
};

const TARGET_ASPECT_RATIO: f64 = 4.0 / 3.0;
const SOURCE_GLYPH_WIDTH: f64 = source_flow::GLYPH_WIDTH;
const CODE_HEADER_GLYPH_WIDTH: f64 = 7.25 * 2.0;
const FILE_HEADER_GLYPH_WIDTH: f64 = 7.25 * 4.0;
const SOURCE_LINE_HEIGHT: f64 = source_flow::LINE_HEIGHT;
const SOURCE_HEADER_HEIGHT: f64 = 40.0;
const CONTAINER_HEADER_HEIGHT: f64 = 40.0;
const FILE_HEADER_HEIGHT: f64 = 64.0;
const DIRECTORY_HEADER_HEIGHT: f64 = 64.0;
const HEADER_GAP: f64 = 6.0;
const FILESYSTEM_HEADER_BORDER_ALLOWANCE: f64 = 4.0;
const NODE_PADDING: f64 = 10.0;
const CONTAINER_PADDING_X: f64 = 16.0;
const GAP: f64 = 8.0;
const HEIGHT_FACTORS: [f64; 13] = [
    0.45, 0.6, 0.75, 0.9, 1.0, 1.15, 1.35, 1.6, 1.9, 2.25, 2.7, 3.2, 4.0,
];

#[derive(Debug, PartialEq, Serialize)]
pub struct PackedMap {
    pub root: MapNode,
    pub aspect_ratio: f64,
    pub world_width: f64,
    pub world_height: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutConfig {
    pub wrap_column: usize,
    pub mode: LayoutMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutMode {
    Structure,
    Source,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct MapNode {
    pub ancestor_ids: Vec<usize>,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub kind: &'static str,
    pub rect: Rect,
    pub header_height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_byte: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_byte: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<SourceFlow>,
    pub children: Vec<MapNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Breadcrumb {
    pub id: usize,
    pub name: String,
    pub path: String,
    pub kind: &'static str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn pack(root: ProjectNode, config: LayoutConfig) -> PackedMap {
    let mut next_id = 0;
    let visual = visual_node(root, &[], &mut next_id);
    let root = pack_node(visual, config);
    let world_width = root.rect.width.max(1.0);
    let world_height = root.rect.height.max(1.0);
    PackedMap {
        root,
        aspect_ratio: world_width / world_height,
        world_width,
        world_height,
    }
}

struct VisualNode {
    ancestor_ids: Vec<usize>,
    breadcrumbs: Vec<Breadcrumb>,
    kind: &'static str,
    source: Option<String>,
    file_source: Option<FileSource>,
    rows: usize,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
    children: Vec<VisualNode>,
}

fn visual_node(node: ProjectNode, ancestors: &[usize], next_id: &mut usize) -> VisualNode {
    let id = *next_id;
    *next_id += 1;
    let kind = kind_name(node.kind);
    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(id);
    let children = node
        .children
        .into_iter()
        .map(|child| visual_node(child, &child_ancestors, next_id))
        .collect();
    let mut visual = VisualNode {
        ancestor_ids: ancestors.to_vec(),
        breadcrumbs: vec![Breadcrumb {
            id,
            name: node.name,
            path: node.path,
            kind,
        }],
        kind,
        source: node.source,
        file_source: node.file_source,
        rows: node.rows,
        start_byte: node.start_byte,
        end_byte: node.end_byte,
        children,
    };

    while visual.source.is_none()
        && visual.file_source.is_none()
        && visual.children.len() == 1
        && visual.children[0].source.is_none()
        && visual.children[0].file_source.is_none()
    {
        let mut child = visual.children.pop().expect("single child");
        visual.breadcrumbs.append(&mut child.breadcrumbs);
        visual.children = child.children;
        visual.file_source = child.file_source;
        visual.start_byte = child.start_byte;
        visual.end_byte = child.end_byte;
    }
    visual
}

fn pack_node(mut node: VisualNode, config: LayoutConfig) -> MapNode {
    let source_width = config.wrap_column as f64 * SOURCE_GLYPH_WIDTH + NODE_PADDING * 2.0;
    let sorts_filesystem_children = node
        .breadcrumbs
        .last()
        .is_some_and(|breadcrumb| breadcrumb.kind == "directory");
    let header_height = if node
        .breadcrumbs
        .iter()
        .any(|breadcrumb| !breadcrumb.name.is_empty())
    {
        header_height(node.kind)
    } else {
        0.0
    };
    if config.mode == LayoutMode::Source
        && let Some(file_source) = node.file_source.take()
    {
        let flow = source_flow::build(
            file_source.text,
            &file_source.highlights,
            file_source.visible_ranges.as_deref(),
            config.wrap_column,
        );
        let column_count = flow.columns.len();
        let content_width =
            column_count as f64 * config.wrap_column as f64 * source_flow::GLYPH_WIDTH
                + column_count.saturating_sub(1) as f64 * source_flow::COLUMN_GAP;
        let content_height = flow
            .columns
            .iter()
            .map(|column| column.units)
            .max()
            .unwrap_or(1) as f64
            * source_flow::LINE_HEIGHT;
        let width = (content_width + CONTAINER_PADDING_X * 2.0)
            .max(header_width(&node.breadcrumbs, node.kind));
        return MapNode {
            ancestor_ids: node.ancestor_ids,
            breadcrumbs: node.breadcrumbs,
            kind: node.kind,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: content_height + header_height + GAP * 2.0,
            },
            header_height,
            source: None,
            start_byte: node.start_byte,
            end_byte: node.end_byte,
            flow: Some(flow),
            children: Vec::new(),
        };
    }
    let mut children = node
        .children
        .into_iter()
        .map(|child| pack_node(child, config))
        .collect::<Vec<_>>();
    sort_children(&mut children, source_width, sorts_filesystem_children);
    let has_source = node.source.is_some();
    let (content_width, height) = if has_source && config.mode == LayoutMode::Structure {
        (
            header_width(&node.breadcrumbs, node.kind),
            header_height.max(1.0),
        )
    } else if has_source {
        (
            source_width,
            header_height + node.rows.max(1) as f64 * SOURCE_LINE_HEIGHT + NODE_PADDING * 2.0,
        )
    } else if children.is_empty() {
        (source_width, header_height + NODE_PADDING * 2.0)
    } else {
        pack_children(&mut children, header_height)
    };
    let width = content_width.max(header_width(&node.breadcrumbs, node.kind));

    MapNode {
        ancestor_ids: node.ancestor_ids,
        breadcrumbs: node.breadcrumbs,
        kind: node.kind,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        },
        header_height,
        source: (config.mode == LayoutMode::Source)
            .then_some(node.source)
            .flatten(),
        start_byte: node.start_byte,
        end_byte: node.end_byte,
        flow: None,
        children,
    }
}

fn pack_children(children: &mut [MapNode], header_height: f64) -> (f64, f64) {
    let total_area = children
        .iter()
        .map(|child| child.rect.width * child.rect.height)
        .sum::<f64>();
    let tallest = children
        .iter()
        .map(|child| child.rect.height)
        .fold(0.0_f64, f64::max);
    let stacked = children.iter().map(|child| child.rect.height).sum::<f64>()
        + GAP * children.len().saturating_sub(1) as f64;
    let ideal = (total_area / TARGET_ASPECT_RATIO).sqrt().max(tallest);
    let mut targets = HEIGHT_FACTORS
        .map(|factor| (ideal * factor).max(tallest))
        .to_vec();
    targets.extend([tallest, stacked]);
    targets.sort_by(f64::total_cmp);
    targets.dedup_by(|left, right| (*left - *right).abs() < 0.01);

    let mut best = None;
    for target in targets {
        let candidate = shelf_candidate(children, target, total_area, header_height);
        if best
            .as_ref()
            .is_none_or(|current: &ShelfCandidate| candidate.score < current.score)
        {
            best = Some(candidate);
        }
    }
    let best = best.expect("non-empty child list");
    for (child, rect) in children.iter_mut().zip(best.rects) {
        child.rect.x = rect.x + CONTAINER_PADDING_X;
        child.rect.y = rect.y + header_height + GAP;
    }
    (
        best.content_width + CONTAINER_PADDING_X * 2.0,
        best.content_height + header_height + GAP * 2.0,
    )
}

struct ShelfCandidate {
    rects: Vec<Rect>,
    content_width: f64,
    content_height: f64,
    score: f64,
}

struct Column {
    indexes: Vec<usize>,
    width: f64,
    height: f64,
}

fn shelf_candidate(
    children: &[MapNode],
    target_height: f64,
    total_area: f64,
    header_height: f64,
) -> ShelfCandidate {
    let mut columns = Vec::<Column>::new();
    for (index, child) in children.iter().enumerate() {
        let next_height = columns.last().map_or(child.rect.height, |column| {
            column.height + GAP + child.rect.height
        });
        if columns
            .last()
            .is_some_and(|column| !column.indexes.is_empty() && next_height > target_height)
        {
            columns.push(Column {
                indexes: Vec::new(),
                width: 0.0,
                height: 0.0,
            });
        }
        if columns.is_empty() {
            columns.push(Column {
                indexes: Vec::new(),
                width: 0.0,
                height: 0.0,
            });
        }
        let column = columns.last_mut().expect("column exists");
        if !column.indexes.is_empty() {
            column.height += GAP;
        }
        column.indexes.push(index);
        column.width = column.width.max(child.rect.width);
        column.height += child.rect.height;
    }

    let content_width =
        columns.iter().map(|column| column.width).sum::<f64>() + GAP * (columns.len() - 1) as f64;
    let content_height = columns
        .iter()
        .map(|column| column.height)
        .fold(0.0_f64, f64::max);
    let mut rects = vec![Rect::default(); children.len()];
    let mut x = 0.0;
    for column in &columns {
        let mut y = 0.0;
        for index in &column.indexes {
            let child = &children[*index];
            rects[*index] = Rect {
                x,
                y,
                width: child.rect.width,
                height: child.rect.height,
            };
            y += child.rect.height + GAP;
        }
        x += column.width + GAP;
    }

    let outer_width = content_width + CONTAINER_PADDING_X * 2.0;
    let outer_height = content_height + header_height + GAP * 2.0;
    let aspect_error = ((outer_width / outer_height) / TARGET_ASPECT_RATIO)
        .ln()
        .abs();
    let waste = if content_width * content_height > 0.0 {
        1.0 - total_area / (content_width * content_height)
    } else {
        0.0
    };
    let mean_height =
        columns.iter().map(|column| column.height).sum::<f64>() / columns.len() as f64;
    let unevenness = columns
        .iter()
        .map(|column| (column.height - mean_height).abs())
        .sum::<f64>()
        / (columns.len() as f64 * content_height.max(1.0));

    ShelfCandidate {
        rects,
        content_width,
        content_height,
        score: aspect_error * 2.0 + waste.max(0.0) * 0.35 + unevenness * 0.1,
    }
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Directory => "directory",
        NodeKind::File => "file",
        NodeKind::Container => "container",
        NodeKind::Source => "source",
    }
}

fn sort_children(children: &mut [MapNode], source_width: f64, files_before_directories: bool) {
    children.sort_by_key(|child| {
        let kind = child
            .breadcrumbs
            .last()
            .map_or(child.kind, |breadcrumb| breadcrumb.kind);
        (
            files_before_directories && kind == "directory",
            (child.rect.width / source_width).ceil() as usize,
        )
    });
}

fn header_height(kind: &str) -> f64 {
    match kind {
        "directory" => DIRECTORY_HEADER_HEIGHT,
        "file" => FILE_HEADER_HEIGHT,
        "container" => CONTAINER_HEADER_HEIGHT,
        "source" => SOURCE_HEADER_HEIGHT,
        _ => unreachable!("known node kind"),
    }
}

fn header_width(breadcrumbs: &[Breadcrumb], kind: &str) -> f64 {
    let characters = breadcrumbs
        .iter()
        .map(|breadcrumb| breadcrumb.name.chars().count())
        .sum::<usize>()
        + breadcrumbs.len().saturating_sub(1);
    let gaps = breadcrumbs.len().saturating_sub(1) * 2;
    let (glyph_width, border_allowance) = match kind {
        "directory" | "file" => (FILE_HEADER_GLYPH_WIDTH, FILESYSTEM_HEADER_BORDER_ALLOWANCE),
        "container" | "source" => (CODE_HEADER_GLYPH_WIDTH, 0.0),
        _ => unreachable!("known node kind"),
    };
    characters.max(1) as f64 * glyph_width
        + gaps as f64 * HEADER_GAP
        + NODE_PADDING * 2.0
        + border_allowance
}

#[cfg(test)]
mod tests {
    use crate::{
        project::{FileSource, NodeKind, ProjectNode},
        syntax::SourceHighlight,
    };

    use super::{
        Breadcrumb, CONTAINER_HEADER_HEIGHT, CONTAINER_PADDING_X, DIRECTORY_HEADER_HEIGHT,
        FILE_HEADER_HEIGHT, GAP, LayoutConfig, LayoutMode, MapNode, Rect, SOURCE_HEADER_HEIGHT,
        SOURCE_LINE_HEIGHT, header_height, header_width, pack, sort_children,
    };

    const CONFIG: LayoutConfig = LayoutConfig {
        wrap_column: 120,
        mode: LayoutMode::Source,
    };
    const STRUCTURE_CONFIG: LayoutConfig = LayoutConfig {
        wrap_column: 120,
        mode: LayoutMode::Structure,
    };

    #[test]
    fn uses_semantic_header_heights() {
        assert_eq!(header_height("source"), SOURCE_HEADER_HEIGHT);
        assert_eq!(header_height("container"), CONTAINER_HEADER_HEIGHT);
        assert_eq!(header_height("file"), FILE_HEADER_HEIGHT);
        assert_eq!(header_height("directory"), DIRECTORY_HEADER_HEIGHT);
    }

    #[test]
    fn packs_ordered_children_without_overlap() {
        let map = pack(
            node(
                "root",
                NodeKind::Directory,
                vec![
                    source("first", 12),
                    source("second", 40),
                    source("third", 8),
                ],
            ),
            CONFIG,
        );

        assert_eq!(
            map.root
                .children
                .iter()
                .map(|child| child.breadcrumbs[0].name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_no_overlap(&map.root.children);
        assert_contained(&map.root);
    }

    #[test]
    fn uses_wider_horizontal_than_vertical_container_insets() {
        let map = pack(
            node("root", NodeKind::Directory, vec![source("only child", 2)]),
            CONFIG,
        );
        let child = &map.root.children[0];

        assert_eq!(child.rect.x, CONTAINER_PADDING_X);
        assert_eq!(child.rect.y, DIRECTORY_HEADER_HEIGHT + GAP);
        assert_eq!(
            map.root.rect.width,
            child.rect.width + CONTAINER_PADDING_X * 2.0
        );
        assert_eq!(
            map.root.rect.height,
            child.rect.height + DIRECTORY_HEADER_HEIGHT + GAP * 2.0
        );
    }

    #[test]
    fn sorts_files_and_folders_by_stable_ascending_width_buckets() {
        let mut children = vec![
            packed_node("narrow file", "file", 150.0),
            packed_node("wide file", "file", 250.0),
            packed_node("same bucket file", "file", 190.0),
            packed_node("wide folder", "directory", 350.0),
            packed_node("narrow folder", "directory", 220.0),
        ];

        sort_children(&mut children, 100.0, true);

        assert_eq!(
            children
                .iter()
                .map(|child| child.breadcrumbs[0].name.as_str())
                .collect::<Vec<_>>(),
            [
                "narrow file",
                "same bucket file",
                "wide file",
                "narrow folder",
                "wide folder"
            ]
        );
    }

    #[test]
    fn sorts_code_elements_by_stable_ascending_width_buckets() {
        let mut children = vec![
            packed_node("medium", "container", 250.0),
            packed_node("narrow", "source", 150.0),
            packed_node("same bucket", "container", 190.0),
            packed_node("wide", "container", 350.0),
        ];

        sort_children(&mut children, 100.0, false);

        assert_eq!(
            children
                .iter()
                .map(|child| child.breadcrumbs[0].name.as_str())
                .collect::<Vec<_>>(),
            ["narrow", "same bucket", "medium", "wide"]
        );
    }

    #[test]
    fn collapses_unary_container_chains_into_breadcrumbs() {
        let map = pack(
            node(
                "root",
                NodeKind::Directory,
                vec![node(
                    "src",
                    NodeKind::Directory,
                    vec![node(
                        "main.rs",
                        NodeKind::File,
                        vec![source("one", 2), source("two", 2)],
                    )],
                )],
            ),
            CONFIG,
        );

        assert_eq!(
            map.root
                .breadcrumbs
                .iter()
                .map(|crumb| crumb.name.as_str())
                .collect::<Vec<_>>(),
            ["root", "src", "main.rs"]
        );
        assert_eq!(map.root.children.len(), 2);
    }

    #[test]
    fn leaves_oversized_source_as_one_card() {
        let map = pack(
            node(
                "root",
                NodeKind::Directory,
                vec![source("long", 2_000), source("short", 2)],
            ),
            CONFIG,
        );

        assert_eq!(map.root.children.len(), 2);
        assert!(map.root.children[0].rect.height > 30_000.0);
    }

    #[test]
    fn structure_mode_uses_header_only_source_cards() {
        let project = node(
            "root",
            NodeKind::Directory,
            vec![source("short", 2), source("a longer title", 2_000)],
        );
        let structure = pack(project.clone(), STRUCTURE_CONFIG);
        let source = pack(project, CONFIG);

        assert_eq!(structure.root.children[0].source, None);
        assert_eq!(structure.root.children[0].rect.height, SOURCE_HEADER_HEIGHT);
        assert!(structure.root.children[0].rect.width < structure.root.children[1].rect.width);
        assert!(source.root.children[0].source.is_some());
        assert_eq!(
            structure.root.children[0].breadcrumbs[0].id,
            source.root.children[0].breadcrumbs[0].id
        );
    }

    #[test]
    fn source_mode_uses_scaled_line_height() {
        let map = pack(
            node("root", NodeKind::Directory, vec![source("code", 3)]),
            CONFIG,
        );

        assert_eq!(
            map.root.children[0].rect.height,
            SOURCE_HEADER_HEIGHT + SOURCE_LINE_HEIGHT * 3.0 + 20.0
        );
    }

    #[test]
    fn source_mode_replaces_file_children_with_continuous_flow() {
        let file_text = "fn one() {}\n\nfn two() {}\n";
        let project = node(
            "root",
            NodeKind::Directory,
            vec![ProjectNode {
                name: "main.rs".into(),
                path: "main.rs".into(),
                kind: NodeKind::File,
                public: false,
                public_members: false,
                declared_type: None,
                trait_impl_target: None,
                scaffold_ranges: Vec::new(),
                source: None,
                file_source: Some(FileSource {
                    text: file_text.into(),
                    highlights: vec![SourceHighlight {
                        lead_start_byte: 0,
                        start_byte: 0,
                        end_byte: "fn one() {".len(),
                        element_end_byte: "fn one() {}".len(),
                    }],
                    visible_ranges: None,
                }),
                children: vec![source("fn one", 1), source("fn two", 1)],
                rows: 0,
                start_byte: None,
                end_byte: None,
            }],
        );
        let structure = pack(project.clone(), STRUCTURE_CONFIG);
        let source_map = pack(project, CONFIG);
        let structure_file = &structure.root.children[0];
        let source_file = &source_map.root.children[0];

        assert_eq!(structure_file.children.len(), 2);
        assert!(structure_file.flow.is_none());
        assert!(source_file.children.is_empty());
        assert_eq!(
            source_file.flow.as_ref().map(|flow| flow.source.as_str()),
            Some(file_text)
        );
    }

    #[test]
    fn structure_mode_keeps_parent_headers_full_width() {
        let map = pack(
            node(
                "a very long directory name",
                NodeKind::Directory,
                vec![source("-", 1)],
            ),
            STRUCTURE_CONFIG,
        );

        assert_eq!(
            map.root.rect.width,
            header_width(&map.root.breadcrumbs, map.root.kind)
        );
    }

    #[test]
    fn produces_deterministic_layouts() {
        let project = node(
            "root",
            NodeKind::Directory,
            (0..20)
                .map(|index| source(&format!("node {index}"), index + 1))
                .collect(),
        );

        assert_eq!(pack(project.clone(), CONFIG), pack(project, CONFIG));
    }

    fn assert_no_overlap(nodes: &[MapNode]) {
        for (index, left) in nodes.iter().enumerate() {
            for right in &nodes[index + 1..] {
                assert!(
                    left.rect.x + left.rect.width <= right.rect.x
                        || right.rect.x + right.rect.width <= left.rect.x
                        || left.rect.y + left.rect.height <= right.rect.y
                        || right.rect.y + right.rect.height <= left.rect.y
                );
            }
        }
    }

    fn assert_contained(node: &MapNode) {
        for child in &node.children {
            assert!(child.rect.x >= 0.0);
            assert!(child.rect.y >= 0.0);
            assert!(child.rect.x + child.rect.width <= node.rect.width);
            assert!(child.rect.y + child.rect.height <= node.rect.height);
            assert_contained(child);
        }
    }

    fn source(name: &str, rows: usize) -> ProjectNode {
        ProjectNode {
            name: name.into(),
            path: name.into(),
            kind: NodeKind::Source,
            public: false,
            public_members: false,
            declared_type: None,
            trait_impl_target: None,
            scaffold_ranges: Vec::new(),
            source: Some(name.into()),
            file_source: None,
            children: Vec::new(),
            rows,
            start_byte: Some(0),
            end_byte: Some(name.len()),
        }
    }

    fn packed_node(name: &str, kind: &'static str, width: f64) -> MapNode {
        MapNode {
            ancestor_ids: Vec::new(),
            breadcrumbs: vec![Breadcrumb {
                id: 0,
                name: name.into(),
                path: name.into(),
                kind,
            }],
            kind,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: 100.0,
            },
            header_height: 0.0,
            source: None,
            start_byte: None,
            end_byte: None,
            flow: None,
            children: Vec::new(),
        }
    }

    fn node(name: &str, kind: NodeKind, children: Vec<ProjectNode>) -> ProjectNode {
        ProjectNode {
            name: name.into(),
            path: name.into(),
            kind,
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
        }
    }
}
