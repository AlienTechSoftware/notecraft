use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use mermaid_rs_renderer::ir::{
    BlockDiagram, BlockNode, EdgeStyleOverride, NodeStyle, PieSlice, QuadrantPoint, XYSeries,
    XYSeriesKind,
};
use mermaid_rs_renderer::layout::validate_layout_invariants;
use mermaid_rs_renderer::{
    DiagramKind, Direction, Edge, EdgeStyle, Graph, LayoutConfig, NodeShape, StateNote,
    StateNotePosition, Subgraph, Theme, compute_layout, render_svg,
};

fn edge(from: &str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        label: Some("label with <xml> & unicode 🦀".to_string()),
        start_label: Some("start".to_string()),
        end_label: Some("end".to_string()),
        directed: true,
        arrow_start: false,
        arrow_end: true,
        arrow_start_kind: None,
        arrow_end_kind: None,
        start_decoration: None,
        end_decoration: None,
        style: EdgeStyle::Dotted,
    }
}

fn has_non_finite_numeric_attribute(svg: &str) -> bool {
    svg.split('"').skip(1).step_by(2).any(|attr_value| {
        attr_value
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
            .any(|token| {
                matches!(
                    token.to_ascii_lowercase().as_str(),
                    "nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
                )
            })
    })
}

fn graph_with_malformed_public_ir(kind: DiagramKind) -> Graph {
    let mut graph = Graph::new();
    graph.kind = kind;
    graph.direction = Direction::LeftRight;
    graph.ensure_node(
        "visible",
        Some("Visible <&>".to_string()),
        Some(NodeShape::Rectangle),
    );
    graph.edges.push(edge("missing_from", "missing_to"));
    graph.edges.push(edge("visible", "missing_to"));
    graph.subgraphs.push(Subgraph {
        id: Some("sg".to_string()),
        label: "Subgraph <&>".to_string(),
        nodes: vec!["visible".to_string(), "missing_member".to_string()],
        direction: Some(Direction::TopDown),
        icon: None,
    });
    graph.state_notes.push(StateNote {
        position: StateNotePosition::RightOf,
        target: "missing_state".to_string(),
        label: "missing target note".to_string(),
    });
    graph
}

#[test]
fn malformed_public_graph_ir_does_not_panic_or_emit_non_finite_svg() {
    let kinds = [
        DiagramKind::Flowchart,
        DiagramKind::Class,
        DiagramKind::State,
        DiagramKind::Er,
        DiagramKind::Requirement,
        DiagramKind::Architecture,
        DiagramKind::Block,
        DiagramKind::Kanban,
    ];
    let theme = Theme::modern();
    let config = LayoutConfig::default();

    for kind in kinds {
        let graph = graph_with_malformed_public_ir(kind);
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let layout = compute_layout(&graph, &theme, &config);
            if let Err(errors) = validate_layout_invariants(&layout) {
                panic!(
                    "{kind:?}: invariant violations:\n{}",
                    errors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n")
                );
            }
            let svg = render_svg(&layout, &theme, &config);
            assert!(svg.contains("<svg"), "{kind:?}: missing svg");
            assert!(
                !has_non_finite_numeric_attribute(&svg),
                "{kind:?}: SVG contains non-finite numeric attribute"
            );
        }));
        assert!(result.is_ok(), "{kind:?}: compute/render panicked");
    }
}

#[test]
fn malformed_block_columns_zero_does_not_panic() {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::Block;
    graph.block = Some(BlockDiagram {
        columns: Some(0),
        nodes: vec![BlockNode {
            id: "A".to_string(),
            span: 1,
            is_space: false,
        }],
    });
    graph.ensure_node("A", Some("A".to_string()), Some(NodeShape::Rectangle));

    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let layout = compute_layout(&graph, &theme, &config);
    validate_layout_invariants(&layout).expect("block layout should stay well formed");
    let svg = render_svg(&layout, &theme, &config);
    assert!(!has_non_finite_numeric_attribute(&svg));
}

#[test]
fn non_finite_public_numeric_data_is_sanitized() {
    let theme = Theme::modern();
    let config = LayoutConfig::default();

    let mut pie = Graph::new();
    pie.kind = DiagramKind::Pie;
    pie.pie_slices = vec![
        PieSlice {
            label: "bad".to_string(),
            value: f32::INFINITY,
        },
        PieSlice {
            label: "nan".to_string(),
            value: f32::NAN,
        },
    ];

    let mut sankey = Graph::new();
    sankey.kind = DiagramKind::Sankey;
    sankey.ensure_node("A", Some("A".to_string()), Some(NodeShape::Rectangle));
    sankey.ensure_node("B", Some("B".to_string()), Some(NodeShape::Rectangle));
    let mut bad_edge = edge("A", "B");
    bad_edge.label = Some("inf".to_string());
    sankey.edges.push(bad_edge);

    let mut treemap = Graph::new();
    treemap.kind = DiagramKind::Treemap;
    treemap.ensure_node("A", Some("A".to_string()), Some(NodeShape::Rectangle));
    treemap.nodes.get_mut("A").unwrap().value = Some(f32::INFINITY);

    let mut quadrant = Graph::new();
    quadrant.kind = DiagramKind::Quadrant;
    quadrant.quadrant.points = vec![
        QuadrantPoint {
            label: "nan".to_string(),
            x: f32::NAN,
            y: 0.5,
        },
        QuadrantPoint {
            label: "bad_y".to_string(),
            x: 0.5,
            y: f32::INFINITY,
        },
    ];

    let mut xychart = Graph::new();
    xychart.kind = DiagramKind::XYChart;
    xychart.xychart.y_axis_min = Some(f32::NAN);
    xychart.xychart.y_axis_max = Some(f32::INFINITY);
    xychart.xychart.x_axis_categories = vec!["a".to_string(), "b".to_string()];
    xychart.xychart.series = vec![
        XYSeries {
            kind: XYSeriesKind::Bar,
            label: None,
            values: vec![f32::NAN, f32::INFINITY],
        },
        XYSeries {
            kind: XYSeriesKind::Line,
            label: None,
            values: vec![f32::NEG_INFINITY, 1.0],
        },
    ];

    let mut styled = Graph::new();
    styled.kind = DiagramKind::Flowchart;
    styled.ensure_node("A", Some("A".to_string()), Some(NodeShape::Rectangle));
    styled.ensure_node("B", Some("B".to_string()), Some(NodeShape::Rectangle));
    styled.edges.push(edge("A", "B"));
    styled.node_styles.insert(
        "A".to_string(),
        NodeStyle {
            stroke_width: Some(f32::NAN),
            ..Default::default()
        },
    );
    styled.edge_styles.insert(
        0,
        EdgeStyleOverride {
            stroke_width: Some(f32::INFINITY),
            ..Default::default()
        },
    );

    for (name, graph) in [
        ("pie", pie),
        ("sankey", sankey),
        ("treemap", treemap),
        ("quadrant", quadrant),
        ("xychart", xychart),
        ("styled", styled),
    ] {
        let layout = compute_layout(&graph, &theme, &config);
        validate_layout_invariants(&layout)
            .unwrap_or_else(|errors| panic!("{name}: invariant errors: {errors:?}"));
        let svg = render_svg(&layout, &theme, &config);
        assert!(
            !has_non_finite_numeric_attribute(&svg),
            "{name}: SVG contains non-finite numeric attribute"
        );
    }
}

// ── Issue regressions ───────────────────────────────────────────────

/// Issue #37: enormous coordinates (for example from one huge unbroken label)
/// used to leave `route_edge_with_avoidance` with an empty candidate list and
/// panic with "index out of bounds: the len is 0 but the index is 0"
/// (src/layout/routing.rs:1994 in v0.2.1, `expect("candidate")` in v0.2.2).
/// The router must fall back to a straight-line route instead of panicking.
#[test]
fn issue_37_offscreen_coordinates_from_huge_label_do_not_panic() {
    let big = "M".repeat(20_000);
    let src = format!("flowchart LR\n  A[{big}] --> B[End]\n  B --> A\n");
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        mermaid_rs_renderer::render(&src).expect("diagram should parse")
    }));
    let svg = result.expect("issue #37 repro: render panicked");
    assert!(svg.contains("<svg"));
    assert!(!has_non_finite_numeric_attribute(&svg));
}

/// Issue #37 companion: degenerate flowcharts that can starve the router of
/// candidates (self loops, overlapping mutual edges, zero-size labels).
#[test]
fn issue_37_degenerate_flowcharts_do_not_panic() {
    let cases = [
        // Self loop only.
        "flowchart TD\n  A --> A\n",
        // Self loop plus mutual edges.
        "flowchart TD\n  A --> A\n  A --> B\n  B --> A\n",
        // Empty labels.
        "flowchart LR\n  A[\" \"] --> B[\" \"]\n  B --> A\n",
        // Duplicate edges stacked on one pair.
        "flowchart LR\n  A --> B\n  A --> B\n  A --> B\n  B --> A\n  B --> A\n",
        // Single node, no edges.
        "flowchart TD\n  A\n",
    ];
    for src in cases {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            mermaid_rs_renderer::render(src).expect("diagram should parse")
        }));
        let svg = result.unwrap_or_else(|_| panic!("render panicked for: {src}"));
        assert!(svg.contains("<svg"), "missing svg for: {src}");
    }
}

/// Issue #116: flowcharts with decision (diamond) nodes panicked in v0.2.2.
/// Cover the reported shape plus hexagons and labeled edges from a diamond.
#[test]
fn issue_116_decision_nodes_render_without_panic() {
    let cases = [
        // The exact reporter snippet.
        "flowchart TD\n  A[Start] --> B{Decision}\n  B -->|Yes| C[End]\n",
        // Diamond with both labeled branches, hexagon, and a cycle back into
        // the diamond.
        "flowchart TD\n  A[Start] --> B{Is it valid?}\n  B -->|Yes| C[Process]\n  B -->|No| D{{Retry queue}}\n  D -->|requeue| B\n  C --> E([Done])\n",
    ];
    for src in cases {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            mermaid_rs_renderer::render(src).expect("diagram should parse")
        }));
        let svg = result.unwrap_or_else(|_| panic!("render panicked for: {src}"));
        assert!(svg.contains("<svg"), "missing svg for: {src}");
        assert!(
            !has_non_finite_numeric_attribute(&svg),
            "bad svg for: {src}"
        );
    }
}

// ── Library-level no-panic guarantee over the fixture corpus ────────

fn collect_mmd_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mmd_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("mmd") {
            out.push(path);
        }
    }
}

/// The public `render()` API must never panic, whatever the input. Render
/// every fixture in `tests/fixtures/**/*.mmd` under `catch_unwind`. Parse
/// errors are fine (they return `Err`); panics are not.
#[test]
fn render_never_panics_on_any_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut fixtures = Vec::new();
    collect_mmd_recursive(&root, &mut fixtures);
    fixtures.sort();
    assert!(!fixtures.is_empty(), "no fixtures found under {root:?}");

    let mut panics = Vec::new();
    for path in &fixtures {
        let Ok(input) = std::fs::read_to_string(path) else {
            continue;
        };
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = mermaid_rs_renderer::render(&input);
        }));
        if result.is_err() {
            panics.push(path.display().to_string());
        }
    }
    assert!(
        panics.is_empty(),
        "render() panicked on {} fixture(s):\n{}",
        panics.len(),
        panics.join("\n")
    );
}
