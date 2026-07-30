use serde::Serialize;

use crate::project::{NodeKind, ProjectNode};

const LINES_PER_BLOCK: usize = 20;
const COLUMNS_PER_LINE: usize = 40;
const GLYPH_WIDTH: f64 = 29.0;
const HORIZONTAL_PADDING: f64 = 20.0;
const A4_HEIGHT_RATIO: f64 = std::f64::consts::SQRT_2;
const BASE_GAP: f64 = 40.0;
const BLOCK_WIDTH: f64 = COLUMNS_PER_LINE as f64 * GLYPH_WIDTH + HORIZONTAL_PADDING * 2.0;
const BLOCK_HEIGHT: f64 = BLOCK_WIDTH * A4_HEIGHT_RATIO;

#[derive(Serialize)]
pub struct PackedMap {
    pub blocks: Vec<MapBlock>,
    pub lines_per_block: usize,
    pub aspect_ratio: f64,
    pub world_width: f64,
    pub world_height: f64,
}

#[derive(Serialize)]
pub struct MapBlock {
    pub rect: Rect,
    pub nodes: Vec<MapNode>,
}

#[derive(Serialize)]
pub struct MapNode {
    pub id: usize,
    pub ancestor_ids: Vec<usize>,
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub lines: Vec<String>,
    pub slot: usize,
    pub continuation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

pub fn pack(root: ProjectNode) -> PackedMap {
    let mut nodes = Vec::new();
    flatten(root, &mut Vec::new(), &mut nodes);
    let mut blocks = paginate(nodes);
    let mut world_width = 1.0_f64;
    let mut world_height = 1.0_f64;

    for (index, block) in blocks.iter_mut().enumerate() {
        let (column, row) = morton_coordinates(index);
        block.rect = Rect {
            x: axis_position(column, BLOCK_WIDTH),
            y: axis_position(row, BLOCK_HEIGHT),
            width: BLOCK_WIDTH,
            height: BLOCK_HEIGHT,
        };
        world_width = world_width.max(block.rect.x + block.rect.width);
        world_height = world_height.max(block.rect.y + block.rect.height);
    }

    PackedMap {
        blocks,
        lines_per_block: LINES_PER_BLOCK,
        aspect_ratio: world_width / world_height,
        world_width,
        world_height,
    }
}

struct DisplayNode {
    id: usize,
    ancestor_ids: Vec<usize>,
    name: String,
    path: String,
    kind: &'static str,
    lines: Vec<String>,
    has_children: bool,
    source: Option<String>,
    columns: usize,
    rows: usize,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
}

fn flatten(node: ProjectNode, ancestors: &mut Vec<usize>, output: &mut Vec<DisplayNode>) {
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
    let lines = wrap_label(&name, COLUMNS_PER_LINE);
    let has_children = !children.is_empty();
    let id = output.len();
    output.push(DisplayNode {
        id,
        ancestor_ids: ancestors.clone(),
        name,
        path,
        kind: match kind {
            NodeKind::Directory => "directory",
            NodeKind::File => "file",
            NodeKind::Chunk => "chunk",
        },
        lines,
        has_children,
        source,
        columns,
        rows,
        start_byte,
        end_byte,
    });
    ancestors.push(id);
    for child in children {
        flatten(child, ancestors, output);
    }
    ancestors.pop();
}

fn paginate(nodes: Vec<DisplayNode>) -> Vec<MapBlock> {
    let mut blocks = Vec::new();
    let mut current = empty_block();
    let mut used_slots = 0;

    for node in nodes {
        if node.lines.len() > LINES_PER_BLOCK {
            if used_slots > 0 {
                blocks.push(current);
                current = empty_block();
                used_slots = 0;
            }
            for (index, lines) in node.lines.chunks(LINES_PER_BLOCK).enumerate() {
                let mut block = empty_block();
                block
                    .nodes
                    .push(map_node(&node, lines.to_vec(), 0, index > 0));
                blocks.push(block);
            }
            continue;
        }

        let remaining = LINES_PER_BLOCK - used_slots;
        let orphaned_header = node.has_children
            && node.lines.len() < LINES_PER_BLOCK
            && node.lines.len() >= remaining;
        if node.lines.len() > remaining || orphaned_header {
            blocks.push(current);
            current = empty_block();
            used_slots = 0;
        }

        let line_count = node.lines.len();
        current
            .nodes
            .push(map_node(&node, node.lines.clone(), used_slots, false));
        used_slots += line_count;
        if used_slots == LINES_PER_BLOCK {
            blocks.push(current);
            current = empty_block();
            used_slots = 0;
        }
    }

    if !current.nodes.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn map_node(node: &DisplayNode, lines: Vec<String>, slot: usize, continuation: bool) -> MapNode {
    MapNode {
        id: node.id,
        ancestor_ids: node.ancestor_ids.clone(),
        name: node.name.clone(),
        path: node.path.clone(),
        kind: node.kind,
        lines,
        slot,
        continuation,
        source: node.source.clone(),
        columns: node.columns,
        rows: node.rows,
        start_byte: node.start_byte,
        end_byte: node.end_byte,
    }
}

fn empty_block() -> MapBlock {
    MapBlock {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: BLOCK_WIDTH,
            height: BLOCK_HEIGHT,
        },
        nodes: Vec::new(),
    }
}

fn wrap_label(label: &str, columns: usize) -> Vec<String> {
    if label.is_empty() {
        return vec![String::new()];
    }

    let mut remaining = label.chars().collect::<Vec<_>>();
    let mut lines = Vec::new();
    while remaining.len() > columns {
        let split = remaining[..columns]
            .iter()
            .rposition(|character| matches!(character, ' ' | '/' | ':' | '_' | '-'))
            .map_or(columns, |index| index + 1);
        lines.push(remaining.drain(..split).collect());
    }
    lines.push(remaining.into_iter().collect());
    lines
}

fn morton_coordinates(index: usize) -> (usize, usize) {
    let mut column = 0;
    let mut row = 0;
    let mut value = index;
    let mut bit = 0;
    while value > 0 {
        column |= (value & 1) << bit;
        value >>= 1;
        row |= (value & 1) << bit;
        value >>= 1;
        bit += 1;
    }
    (column, row)
}

fn axis_position(index: usize, block_extent: f64) -> f64 {
    let gap_units = index * 2 - index.count_ones() as usize;
    index as f64 * block_extent + gap_units as f64 * BASE_GAP
}

#[cfg(test)]
mod tests {
    use crate::project::{NodeKind, ProjectNode};

    use super::{
        BASE_GAP, BLOCK_HEIGHT, BLOCK_WIDTH, COLUMNS_PER_LINE, DisplayNode, LINES_PER_BLOCK,
        axis_position, morton_coordinates, pack, paginate, wrap_label,
    };

    #[test]
    fn flattens_nodes_in_preorder() {
        let map = pack(directory(
            "root",
            vec![
                file("main.rs", vec![chunk("fn main"), chunk("fn load")]),
                directory("src", vec![file("lib.rs", vec![chunk("struct App")])]),
            ],
        ));
        let names = map
            .blocks
            .iter()
            .flat_map(|block| &block.nodes)
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "root",
                "main.rs",
                "fn main",
                "fn load",
                "src",
                "lib.rs",
                "struct App"
            ]
        );
        let nodes = map
            .blocks
            .iter()
            .flat_map(|block| &block.nodes)
            .collect::<Vec<_>>();
        assert_eq!(nodes[0].ancestor_ids, Vec::<usize>::new());
        assert_eq!(nodes[1].ancestor_ids, [0]);
        assert_eq!(nodes[2].ancestor_ids, [0, 1]);
        assert_eq!(nodes[5].ancestor_ids, [0, 4]);
        assert_eq!(nodes[6].ancestor_ids, [0, 4, 5]);
    }

    #[test]
    fn wraps_at_semantic_separators_and_hard_boundaries() {
        assert_eq!(
            wrap_label("alpha_beta_gamma", 6),
            ["alpha_", "beta_", "gamma"]
        );
        assert_eq!(wrap_label("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap_label("", COLUMNS_PER_LINE), [""]);
    }

    #[test]
    fn keeps_wrapped_nodes_and_headers_together() {
        let mut nodes = (0..19)
            .map(|id| display_node(id, vec!["line"], false))
            .collect::<Vec<_>>();
        nodes.push(display_node(19, vec!["two", "lines"], false));
        nodes.push(display_node(20, vec!["header"], true));
        let blocks = paginate(nodes);

        assert_eq!(blocks[0].nodes.len(), 19);
        assert_eq!(blocks[1].nodes[0].slot, 0);
        assert_eq!(blocks[1].nodes[0].lines, ["two", "lines"]);
        assert_eq!(blocks[1].nodes[1].slot, 2);

        let mut orphan_nodes = (0..19)
            .map(|id| display_node(id, vec!["line"], false))
            .collect::<Vec<_>>();
        orphan_nodes.push(display_node(19, vec!["header"], true));
        orphan_nodes.push(display_node(20, vec!["child"], false));
        let orphan_blocks = paginate(orphan_nodes);

        assert_eq!(orphan_blocks[0].nodes.len(), 19);
        assert_eq!(orphan_blocks[1].nodes[0].slot, 0);
        assert_eq!(orphan_blocks[1].nodes[1].slot, 1);
    }

    #[test]
    fn splits_only_nodes_larger_than_a_block() {
        let lines = (0..LINES_PER_BLOCK + 1)
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let blocks = paginate(vec![display_node(0, lines, false)]);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].nodes[0].lines.len(), LINES_PER_BLOCK);
        assert_eq!(blocks[1].nodes[0].lines.len(), 1);
        assert!(!blocks[0].nodes[0].continuation);
        assert!(blocks[1].nodes[0].continuation);
    }

    #[test]
    fn places_blocks_in_stable_morton_order_with_hierarchical_gaps() {
        assert_eq!(
            (0..8).map(morton_coordinates).collect::<Vec<_>>(),
            [
                (0, 0),
                (1, 0),
                (0, 1),
                (1, 1),
                (2, 0),
                (3, 0),
                (2, 1),
                (3, 1)
            ]
        );
        assert_eq!(axis_position(1, BLOCK_WIDTH), BLOCK_WIDTH + BASE_GAP);
        assert_eq!(
            axis_position(2, BLOCK_WIDTH),
            BLOCK_WIDTH * 2.0 + BASE_GAP * 3.0
        );
        assert_eq!(
            axis_position(4, BLOCK_HEIGHT),
            BLOCK_HEIGHT * 4.0 + BASE_GAP * 7.0
        );
    }

    fn display_node(id: usize, lines: Vec<impl Into<String>>, has_children: bool) -> DisplayNode {
        DisplayNode {
            id,
            ancestor_ids: Vec::new(),
            name: id.to_string(),
            path: id.to_string(),
            kind: if has_children { "file" } else { "chunk" },
            lines: lines.into_iter().map(Into::into).collect(),
            has_children,
            source: None,
            columns: 0,
            rows: 0,
            start_byte: None,
            end_byte: None,
        }
    }

    fn directory(name: &str, children: Vec<ProjectNode>) -> ProjectNode {
        project_node(name, NodeKind::Directory, children)
    }

    fn file(name: &str, children: Vec<ProjectNode>) -> ProjectNode {
        project_node(name, NodeKind::File, children)
    }

    fn chunk(name: &str) -> ProjectNode {
        project_node(name, NodeKind::Chunk, Vec::new())
    }

    fn project_node(name: &str, kind: NodeKind, children: Vec<ProjectNode>) -> ProjectNode {
        ProjectNode {
            name: name.into(),
            path: name.into(),
            kind,
            source: None,
            children,
            columns: 0,
            rows: 0,
            weight: 1.0,
            start_byte: None,
            end_byte: None,
        }
    }
}
