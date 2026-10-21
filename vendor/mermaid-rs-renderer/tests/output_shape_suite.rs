//! Output-shape regression guards (benchmark suite domain: performance/output).
//!
//! Bloated edge geometry (extra points, spurious bends) is both a visual
//! defect and a downstream-performance cost (larger SVG, slower browser
//! paint). It is also exactly how the `on_segment` epsilon bug manifested: a
//! straight chain silently grew dozens of zig-zag bends while other metrics
//! looked fine. These guards assert the output stays tight for cases whose
//! optimal geometry is known, so that class of regression fails fast.

use std::collections::BTreeMap;

use mermaid_rs_renderer::config::LayoutConfig;
use mermaid_rs_renderer::layout::{Layout, compute_layout};
use mermaid_rs_renderer::parser::parse_mermaid;
use mermaid_rs_renderer::theme::Theme;

fn layout_of(input: &str) -> Layout {
    let parsed = parse_mermaid(input).expect("parse");
    compute_layout(&parsed.graph, &Theme::modern(), &LayoutConfig::default())
}

/// Count visually real bends (direction changes), ignoring collinear midpoints.
fn real_bends(layout: &Layout) -> usize {
    let mut total = 0;
    for edge in &layout.edges {
        let p = &edge.points;
        for w in p.windows(3) {
            let (dx1, dy1) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            let (dx2, dy2) = (w[2].0 - w[1].0, w[2].1 - w[1].1);
            let cross = dx1 * dy2 - dy1 * dx2;
            if cross.abs() > 1e-3 {
                total += 1;
            }
        }
    }
    total
}

/// Total edge points across the layout (a proxy for SVG path bloat).
fn total_edge_points(layout: &Layout) -> usize {
    layout.edges.iter().map(|e| e.points.len()).sum()
}

#[test]
fn straight_chains_stay_bend_free() {
    // A pure A->B->C->... chain in either orientation has an optimal layout
    // with zero bends. This directly guards the on_segment-style regression
    // where collinear edges were mistaken for crossings and bent.
    for dir in ["LR", "TD", "RL", "BT"] {
        for n in [3usize, 5, 10, 20] {
            let mut src = format!("flowchart {dir}\n");
            for i in 1..=n {
                src.push_str(&format!("  N{i}[Node {i}]\n"));
            }
            for i in 1..n {
                src.push_str(&format!("  N{i} --> N{}\n", i + 1));
            }
            let layout = layout_of(&src);
            let bends = real_bends(&layout);
            assert_eq!(
                bends, 0,
                "{dir} chain of {n} nodes should have 0 bends, got {bends}"
            );
        }
    }
}

#[test]
fn chain_edges_have_minimal_points() {
    // Each straight chain edge should be exactly two points (start, end).
    let layout = layout_of("flowchart LR\n  A --> B\n  B --> C\n  C --> D\n");
    let points = total_edge_points(&layout);
    assert_eq!(
        points, 6,
        "3 straight edges should have 6 points total, got {points}"
    );
}

#[test]
fn balanced_tree_is_symmetric() {
    // A symmetric binary fan-out should place the two children symmetrically
    // about the parent on the cross axis. Guards a class of placement bias.
    let layout = layout_of("flowchart TD\n  Root --> L\n  Root --> R\n");
    let nodes: BTreeMap<_, _> = layout
        .nodes
        .iter()
        .map(|(id, n)| (id.clone(), (n.x + n.width * 0.5, n.y + n.height * 0.5)))
        .collect();
    let (rx, _ry) = nodes["Root"];
    let (lx, _) = nodes["L"];
    let (rrx, _) = nodes["R"];
    let left_off = (lx - rx).abs();
    let right_off = (rrx - rx).abs();
    assert!(
        (left_off - right_off).abs() <= 4.0,
        "children should be symmetric about Root: left offset {left_off:.1}, right offset {right_off:.1}"
    );
}
