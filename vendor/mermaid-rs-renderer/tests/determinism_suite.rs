//! Determinism and stability gate (benchmark suite domain 3).
//!
//! A layout engine that produces different output for the same input, or
//! wildly different output for a one-element change, is unusable in version
//! control and diff review. This suite enforces two properties:
//!
//!   * Determinism: rendering the same source twice (and parsing it twice)
//!     yields byte-identical SVG and identical layout geometry. Any reliance
//!     on hashmap iteration order, time, or uninitialised state shows up here.
//!
//!   * Incremental stability: appending one new node/edge to a diagram must
//!     not relayout the whole thing. We measure how far the *pre-existing*
//!     nodes move and assert the median displacement stays small. This drives
//!     the "node displacement vs prior layout" objective that was declared in
//!     docs/layout_objective.md but never benchmarked.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mermaid_rs_renderer::config::LayoutConfig;
use mermaid_rs_renderer::layout::{Layout, compute_layout};
use mermaid_rs_renderer::parser::parse_mermaid;
use mermaid_rs_renderer::render::render_svg;
use mermaid_rs_renderer::theme::Theme;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_mmd(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mmd(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("mmd") {
            out.push(path);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let root = repo_root();
    let mut out = Vec::new();
    for sub in [
        "tests/fixtures",
        "docs/comparison_sources",
        "benches/fixtures",
    ] {
        collect_mmd(&root.join(sub), &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn fixture_name(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn render_once(input: &str) -> Option<(Layout, String)> {
    let parsed = parse_mermaid(input).ok()?;
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);
    Some((layout, svg))
}

/// Compact geometric signature of a layout: node rects and edge points.
/// Used to compare two layouts for exact geometric equality without depending
/// on SVG formatting.
fn layout_signature(layout: &Layout) -> String {
    let mut s = String::new();
    for (id, n) in &layout.nodes {
        s.push_str(&format!(
            "N {id} {:.4} {:.4} {:.4} {:.4}\n",
            n.x, n.y, n.width, n.height
        ));
    }
    for (idx, e) in layout.edges.iter().enumerate() {
        s.push_str(&format!("E {idx} {}->{}", e.from, e.to));
        for (x, y) in &e.points {
            s.push_str(&format!(" {x:.4},{y:.4}"));
        }
        s.push('\n');
    }
    s
}

#[test]
fn rendering_is_deterministic_for_every_fixture() {
    let mut failures = Vec::new();
    for path in corpus() {
        let Ok(input) = fs::read_to_string(&path) else {
            continue;
        };
        let Some((layout_a, svg_a)) = render_once(&input) else {
            continue; // parse-failure fixtures are covered elsewhere
        };
        let Some((layout_b, svg_b)) = render_once(&input) else {
            failures.push(format!("{}: second render failed", fixture_name(&path)));
            continue;
        };
        if svg_a != svg_b {
            failures.push(format!(
                "{}: SVG not byte-identical across runs",
                fixture_name(&path)
            ));
        }
        let sig_a = layout_signature(&layout_a);
        let sig_b = layout_signature(&layout_b);
        if sig_a != sig_b {
            failures.push(format!(
                "{}: layout geometry differs across runs",
                fixture_name(&path)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "\n{} determinism failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ── Incremental stability ───────────────────────────────────────────

/// Median displacement of the nodes shared between two layouts.
fn median_shared_displacement(before: &Layout, after: &Layout) -> Option<f32> {
    let after_nodes: BTreeMap<&str, (f32, f32)> = after
        .nodes
        .iter()
        .map(|(id, n)| (id.as_str(), (n.x + n.width * 0.5, n.y + n.height * 0.5)))
        .collect();
    let mut deltas: Vec<f32> = Vec::new();
    for (id, n) in &before.nodes {
        if let Some(&(ax, ay)) = after_nodes.get(id.as_str()) {
            let bx = n.x + n.width * 0.5;
            let by = n.y + n.height * 0.5;
            deltas.push(((ax - bx).powi(2) + (ay - by).powi(2)).sqrt());
        }
    }
    if deltas.is_empty() {
        return None;
    }
    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(deltas[deltas.len() / 2])
}

/// Appending a leaf node to a simple chain flowchart should leave the existing
/// nodes essentially where they were. This is a focused, deterministic probe
/// of incremental stability rather than a full mutation corpus (which needs a
/// public incremental-layout API to be meaningful).
#[test]
fn appending_a_leaf_node_barely_moves_existing_nodes() {
    let base = "flowchart TD\n  A[Start] --> B[Middle]\n  B --> C[End]\n";
    let grown = "flowchart TD\n  A[Start] --> B[Middle]\n  B --> C[End]\n  C --> D[Extra]\n";

    let (before, _) = render_once(base).expect("base renders");
    let (after, _) = render_once(grown).expect("grown renders");

    let median = median_shared_displacement(&before, &after).expect("shared nodes");
    // The shared chain A->B->C should not be shoved around by adding a leaf.
    // Allow a modest tolerance for rank-gap/centering adjustments.
    assert!(
        median <= 40.0,
        "adding a leaf node moved existing nodes by median {median:.1}px (expected <= 40)"
    );
}

/// A pure determinism self-test on a tiny fixture, so a regression in the
/// determinism machinery itself is caught fast without scanning the corpus.
#[test]
fn determinism_self_test_tiny() {
    let input = "flowchart LR\n  A --> B\n  B --> C\n";
    let (la, sa) = render_once(input).unwrap();
    let (lb, sb) = render_once(input).unwrap();
    assert_eq!(sa, sb, "tiny SVG must be byte-identical");
    assert_eq!(
        layout_signature(&la),
        layout_signature(&lb),
        "tiny layout must be identical"
    );
}
