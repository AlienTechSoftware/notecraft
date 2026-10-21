//! Integration coverage for the opt-in aspect-ratio serpentine band fold.
//!
//! The fold is keyed entirely off `LayoutConfig::preferred_aspect_ratio`:
//! with no goal set the layout must be byte-for-byte identical to the
//! pre-fold pipeline, and with a goal set an over-wide chain must wrap into
//! bands that land near the goal while keeping edges attached and nodes
//! non-overlapping.

use std::collections::BTreeMap;

use mermaid_rs_renderer::config::LayoutConfig;
use mermaid_rs_renderer::layout::{Layout, compute_layout, validate_layout_invariants};
use mermaid_rs_renderer::parser::parse_mermaid;
use mermaid_rs_renderer::theme::Theme;

const CHAIN_LR: &str = "flowchart LR\n  A[Start] --> B[Parse] --> C[Layout] --> D[Route] --> E[Render] --> F[Done]\n";
const GOAL_4_3: f32 = 4.0 / 3.0;

fn layout_with_goal(source: &str, goal: Option<f32>) -> Layout {
    let parsed = parse_mermaid(source).expect("fixture should parse");
    let mut config = LayoutConfig::default();
    config.preferred_aspect_ratio = goal;
    compute_layout(&parsed.graph, &Theme::mermaid_default(), &config)
}

fn visible_node_bounds(layout: &Layout) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for node in layout.nodes.values() {
        if node.hidden {
            continue;
        }
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    assert!(min_x != f32::MAX, "layout has no visible nodes");
    (min_x, min_y, max_x, max_y)
}

fn node_aspect_ratio(layout: &Layout) -> f32 {
    let (min_x, min_y, max_x, max_y) = visible_node_bounds(layout);
    (max_x - min_x).max(1.0) / (max_y - min_y).max(1.0)
}

/// Distance from `point` to the boundary of the node's axis-aligned box.
fn rect_boundary_distance(point: (f32, f32), node: &mermaid_rs_renderer::NodeLayout) -> f32 {
    let (px, py) = point;
    let (x0, y0, x1, y1) = (node.x, node.y, node.x + node.width, node.y + node.height);
    let inside = px >= x0 && px <= x1 && py >= y0 && py <= y1;
    if inside {
        // Distance to the nearest side (0 when exactly on the boundary).
        (px - x0).min(x1 - px).min(py - y0).min(y1 - py)
    } else {
        let dx = (x0 - px).max(0.0).max(px - x1);
        let dy = (y0 - py).max(0.0).max(py - y1);
        (dx * dx + dy * dy).sqrt()
    }
}

fn assert_edges_connected(layout: &Layout, context: &str) {
    for edge in &layout.edges {
        let from = layout
            .nodes
            .get(&edge.from)
            .unwrap_or_else(|| panic!("{context}: missing node {}", edge.from));
        let to = layout
            .nodes
            .get(&edge.to)
            .unwrap_or_else(|| panic!("{context}: missing node {}", edge.to));
        let first = *edge.points.first().expect("edge has points");
        let last = *edge.points.last().expect("edge has points");
        let start_err = rect_boundary_distance(first, from);
        let end_err = rect_boundary_distance(last, to);
        assert!(
            start_err <= 1.5,
            "{context}: edge {}->{} start point {:?} is {}px off {}'s boundary",
            edge.from,
            edge.to,
            first,
            start_err,
            edge.from
        );
        assert!(
            end_err <= 1.5,
            "{context}: edge {}->{} end point {:?} is {}px off {}'s boundary",
            edge.from,
            edge.to,
            last,
            end_err,
            edge.to
        );
    }
}

fn assert_no_node_overlaps(layout: &Layout, context: &str) {
    let nodes: Vec<_> = layout.nodes.values().filter(|node| !node.hidden).collect();
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let (a, b) = (nodes[i], nodes[j]);
            let overlap_x = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
            let overlap_y = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
            assert!(
                overlap_x <= 0.5 || overlap_y <= 0.5,
                "{context}: nodes {} and {} overlap by {}x{}",
                a.id,
                b.id,
                overlap_x,
                overlap_y
            );
        }
    }
}

/// Serialize the geometry relevant to regression comparison.
fn serialize_layout(layout: &Layout) -> String {
    let mut out = String::new();
    out.push_str(&format!("size {}x{}\n", layout.width, layout.height));
    let nodes: BTreeMap<_, _> = layout.nodes.iter().collect();
    for (id, node) in nodes {
        out.push_str(&format!(
            "node {id} {} {} {} {}\n",
            node.x, node.y, node.width, node.height
        ));
    }
    for edge in &layout.edges {
        out.push_str(&format!("edge {}->{}", edge.from, edge.to));
        for (x, y) in &edge.points {
            out.push_str(&format!(" {x},{y}"));
        }
        out.push('\n');
    }
    out
}

#[test]
fn linear_lr_chain_wraps_toward_goal() {
    let layout = layout_with_goal(CHAIN_LR, Some(GOAL_4_3));
    let ratio = node_aspect_ratio(&layout);
    // The unfolded chain is ~11:1 over node bounds. Within 2x of the goal
    // means [goal/2, goal*2]; the fold plus the stretch refiner should land
    // well inside that.
    assert!(
        ratio >= GOAL_4_3 / 2.0 && ratio <= GOAL_4_3 * 2.0,
        "wrapped chain aspect {ratio} should be within 2x of goal {GOAL_4_3}"
    );

    validate_layout_invariants(&layout).expect("wrapped layout should satisfy invariants");
    assert_edges_connected(&layout, "wrapped chain");
    assert_no_node_overlaps(&layout, "wrapped chain");
    for node in layout.nodes.values() {
        assert!(node.x.is_finite() && node.y.is_finite(), "non-finite node");
    }
}

#[test]
fn no_goal_keeps_wide_chain_wide() {
    let layout = layout_with_goal(CHAIN_LR, None);
    let ratio = node_aspect_ratio(&layout);
    assert!(
        ratio > 5.0,
        "without a goal the LR chain must stay a single wide band, got {ratio}"
    );
    assert_edges_connected(&layout, "no-goal chain");
}

#[test]
fn no_goal_layout_identical_to_pre_fold_pipeline() {
    // The fold must be a true no-op without preferred_aspect_ratio: the
    // serialized geometry of two renders (fresh parses) must be identical,
    // and folding-related code must not perturb any coordinate.
    let a = serialize_layout(&layout_with_goal(CHAIN_LR, None));
    let b = serialize_layout(&layout_with_goal(CHAIN_LR, None));
    assert_eq!(a, b, "layout must be deterministic without a goal");

    // Golden regression, captured from the pre-fold pipeline (master) and
    // verified bit-identical against the post-fold pipeline at introduction.
    // Uses fast text metrics so the golden is font-independent and portable
    // across machines. Regenerate with:
    //   UPDATE_ASPECT_FOLD_GOLDEN=1 cargo test --test aspect_fold_suite
    let parsed = parse_mermaid(CHAIN_LR).expect("fixture should parse");
    let mut config = LayoutConfig::default();
    config.fast_text_metrics = true;
    let layout = compute_layout(&parsed.graph, &Theme::mermaid_default(), &config);
    let serialized = serialize_layout(&layout);

    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flowchart/aspect_chain.layout.golden.txt");
    if std::env::var_os("UPDATE_ASPECT_FOLD_GOLDEN").is_some() {
        std::fs::write(&golden_path, &serialized).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&golden_path).expect(
        "golden file missing; run UPDATE_ASPECT_FOLD_GOLDEN=1 cargo test --test aspect_fold_suite",
    );
    assert_eq!(
        serialized, expected,
        "None-goal layout drifted from the pre-fold golden; if an unrelated \
         layout change caused this, regenerate with UPDATE_ASPECT_FOLD_GOLDEN=1"
    );
}

#[test]
fn wrap_with_back_edge_stays_clean() {
    let source = format!("{CHAIN_LR}  F --> A\n");
    let layout = layout_with_goal(&source, Some(GOAL_4_3));
    validate_layout_invariants(&layout).expect("back-edge layout should satisfy invariants");
    assert_edges_connected(&layout, "wrap with back edge");
    assert_no_node_overlaps(&layout, "wrap with back edge");
    let ratio = node_aspect_ratio(&layout);
    assert!(
        ratio <= GOAL_4_3 * 2.5,
        "back edge should not prevent wrapping entirely, got {ratio}"
    );
}

#[test]
fn feeder_cross_band_edge_stays_connected() {
    let source = format!("{CHAIN_LR}  G[Feeder] --> C\n");
    let layout = layout_with_goal(&source, Some(GOAL_4_3));
    validate_layout_invariants(&layout).expect("feeder layout should satisfy invariants");
    assert_edges_connected(&layout, "feeder cross band");
    assert_no_node_overlaps(&layout, "feeder cross band");
}

#[test]
fn rl_direction_wraps_too() {
    let source = CHAIN_LR.replace("flowchart LR", "flowchart RL");
    let layout = layout_with_goal(&source, Some(GOAL_4_3));
    let ratio = node_aspect_ratio(&layout);
    assert!(
        ratio <= GOAL_4_3 * 2.0,
        "RL chain should wrap via canonical coordinates, got {ratio}"
    );
    assert_edges_connected(&layout, "RL wrap");
    assert_no_node_overlaps(&layout, "RL wrap");
}

#[test]
fn vertical_chain_does_not_fold() {
    // TD chains are out of scope for phase 1: the fold must bail and leave
    // the stretch pass as the only aspect mechanism.
    let source = CHAIN_LR.replace("flowchart LR", "flowchart TD");
    let with_goal = layout_with_goal(&source, Some(4.0));
    let without_goal = layout_with_goal(&source, None);
    // Node x-order must be unchanged (no serpentine reordering).
    let order = |layout: &Layout| {
        let mut ids: Vec<_> = layout
            .nodes
            .values()
            .filter(|node| !node.hidden)
            .map(|node| (node.id.clone(), node.y))
            .collect();
        ids.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        ids.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
    };
    assert_eq!(
        order(&with_goal),
        order(&without_goal),
        "vertical chains must not be folded in phase 1"
    );
    assert_edges_connected(&with_goal, "TD with goal");
}

#[test]
fn subgraph_chain_not_folded() {
    let source = "flowchart LR\n  subgraph S\n    A --> B --> C --> D --> E --> F\n  end\n";
    let with_goal = layout_with_goal(source, Some(GOAL_4_3));
    let without_goal = layout_with_goal(source, None);
    // The fold must bail; relative node geometry matches the stretch-only
    // baseline (the stretch pass scales uniformly, so normalized relative
    // positions along x are preserved).
    let xs = |layout: &Layout| {
        let mut xs: Vec<_> = layout
            .nodes
            .values()
            .filter(|node| !node.hidden)
            .map(|node| (node.id.clone(), node.x))
            .collect();
        xs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        xs.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
    };
    assert_eq!(
        xs(&with_goal),
        xs(&without_goal),
        "subgraph chains must keep their single-band order in phase 1"
    );
}

#[test]
fn goal_render_is_deterministic() {
    let a = serialize_layout(&layout_with_goal(CHAIN_LR, Some(GOAL_4_3)));
    let b = serialize_layout(&layout_with_goal(CHAIN_LR, Some(GOAL_4_3)));
    assert_eq!(a, b, "folded layout must be deterministic");
}
