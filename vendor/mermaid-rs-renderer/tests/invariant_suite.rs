use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mermaid_rs_renderer::ir::{BlockDiagram, QuadrantPoint};
use mermaid_rs_renderer::layout::{
    DiagramData, EdgeLayout, Layout, QuadrantLayout, QuadrantPointLayout, TextBlock,
    flowchart_quality_metrics, validate_layout_invariants,
};
use mermaid_rs_renderer::{
    DiagramKind, EdgeStyle, Graph, LayoutConfig, Theme, compute_layout, parse_mermaid, render_svg,
};

fn empty_text_block() -> TextBlock {
    TextBlock {
        lines: vec![String::new()],
        width: 0.0,
        height: 0.0,
    }
}

fn collect_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut fixtures = Vec::new();
    collect_fixtures_recursive(root, &mut fixtures);
    fixtures.sort();
    fixtures
}

fn collect_fixtures_recursive(dir: &Path, fixtures: &mut Vec<PathBuf>) {
    for entry in
        std::fs::read_dir(dir).unwrap_or_else(|err| panic!("read_dir {}: {err}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|err| panic!("read_dir entry {}: {err}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_fixtures_recursive(&path, fixtures);
        } else if path.extension().is_some_and(|ext| ext == "mmd") {
            fixtures.push(path);
        }
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

#[test]
fn malformed_block_columns_zero_does_not_panic() {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::Block;
    graph.block = Some(BlockDiagram {
        columns: Some(0),
        nodes: Vec::new(),
    });

    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    validate_layout_invariants(&layout).expect("layout should remain valid");
}

#[test]
fn quadrant_layout_sanitizes_non_finite_direct_ir_points() {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::Quadrant;
    graph.quadrant.points.push(QuadrantPoint {
        label: "bad".to_string(),
        x: f32::NAN,
        y: f32::INFINITY,
    });

    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    validate_layout_invariants(&layout).expect("non-finite input should be sanitized in layout");
}

#[test]
fn invariants_reject_non_finite_diagram_specific_geometry() {
    let layout = Layout {
        kind: DiagramKind::Quadrant,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        width: 100.0,
        height: 100.0,
        diagram: DiagramData::Quadrant(QuadrantLayout {
            title: None,
            title_y: 0.0,
            x_axis_left: None,
            x_axis_right: None,
            y_axis_bottom: None,
            y_axis_top: None,
            quadrant_labels: [None, None, None, None],
            points: vec![QuadrantPointLayout {
                label: empty_text_block(),
                x: f32::NAN,
                y: 1.0,
                color: "#000".to_string(),
            }],
            grid_x: 0.0,
            grid_y: 0.0,
            grid_width: 100.0,
            grid_height: 100.0,
        }),
    };

    let errors = validate_layout_invariants(&layout).expect_err("NaN point must fail invariants");
    assert!(
        errors
            .iter()
            .any(|error| error.path.contains("quadrant.points[0].x")),
        "expected quadrant point x error, got {errors:?}"
    );
}

#[test]
fn invariants_reject_edges_with_fewer_than_two_points() {
    let layout = Layout {
        kind: DiagramKind::Flowchart,
        nodes: BTreeMap::new(),
        edges: vec![EdgeLayout {
            from: "A".to_string(),
            to: "B".to_string(),
            label: None,
            start_label: None,
            end_label: None,
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            points: vec![(0.0, 0.0)],
            directed: true,
            arrow_start: false,
            arrow_end: true,
            arrow_start_kind: None,
            arrow_end_kind: None,
            start_decoration: None,
            end_decoration: None,
            style: EdgeStyle::Solid,
            override_style: Default::default(),
        }],
        subgraphs: Vec::new(),
        width: 100.0,
        height: 100.0,
        diagram: DiagramData::Graph {
            state_notes: Vec::new(),
        },
    };

    let errors = validate_layout_invariants(&layout).expect_err("short edge path must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.path.contains("edges[0:A->B].points")),
        "expected edge points error, got {errors:?}"
    );
}

#[test]
fn flowchart_quality_metrics_report_hard_violations_and_score() {
    let parsed = parse_mermaid("flowchart LR; A --> B").expect("parse failed");
    let layout = compute_layout(&parsed.graph, &Theme::modern(), &LayoutConfig::default());
    let metrics = flowchart_quality_metrics(&layout).expect("expected flowchart quality metrics");

    assert_eq!(metrics.edge_count, 1);
    assert_eq!(metrics.hard_violation_count(), 0);
    assert!(metrics.path_length > 0.0);
    assert!(metrics.quality_score.is_finite());
}

#[test]
fn flowchart_quality_metrics_are_flowchart_only() {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::Pie;
    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    assert!(flowchart_quality_metrics(&layout).is_none());
}

#[test]
fn all_repository_fixtures_satisfy_layout_invariants() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut fixtures = collect_fixtures(&manifest.join("tests/fixtures"));
    fixtures.extend(collect_fixtures(&manifest.join("benches/fixtures")));
    fixtures.extend(collect_fixtures(&manifest.join("docs/comparison_sources")));
    fixtures.sort();

    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let mut failures = Vec::new();

    for path in fixtures {
        let rel = path
            .strip_prefix(manifest)
            .unwrap_or(&path)
            .display()
            .to_string();
        let input = match std::fs::read_to_string(&path) {
            Ok(input) => input,
            Err(err) => {
                failures.push(format!("{rel}: read failed: {err}"));
                continue;
            }
        };
        let parsed = match parse_mermaid(&input) {
            Ok(parsed) => parsed,
            Err(err) => {
                failures.push(format!("{rel}: parse failed: {err}"));
                continue;
            }
        };
        let layout = compute_layout(&parsed.graph, &theme, &config);
        if let Err(errors) = validate_layout_invariants(&layout) {
            failures.push(format!(
                "{rel}: layout invariant violations:\n{}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
            continue;
        }
        let svg = render_svg(&layout, &theme, &config);
        if !svg.contains("<svg")
            || !svg.contains("</svg>")
            || has_non_finite_numeric_attribute(&svg)
        {
            failures.push(format!("{rel}: invalid SVG output"));
        }
    }

    assert!(
        failures.is_empty(),
        "fixture invariant failures ({}):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn flowchart_fixtures_have_zero_hard_routing_violations() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut fixtures = collect_fixtures(&manifest.join("tests/fixtures"));
    fixtures.extend(collect_fixtures(&manifest.join("benches/fixtures")));
    fixtures.extend(collect_fixtures(&manifest.join("docs/comparison_sources")));
    fixtures.sort();

    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for path in fixtures {
        let rel = path
            .strip_prefix(manifest)
            .unwrap_or(&path)
            .display()
            .to_string();
        let input = match std::fs::read_to_string(&path) {
            Ok(input) => input,
            Err(err) => {
                failures.push(format!("{rel}: read failed: {err}"));
                continue;
            }
        };
        let parsed = match parse_mermaid(&input) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if parsed.graph.kind != DiagramKind::Flowchart {
            continue;
        }

        checked += 1;
        let layout = compute_layout(&parsed.graph, &theme, &config);
        let Some(metrics) = flowchart_quality_metrics(&layout) else {
            failures.push(format!("{rel}: missing flowchart metrics"));
            continue;
        };
        if metrics.hard_violation_count() != 0 || metrics.geometry_debt_count() != 0 {
            failures.push(format!(
                "{rel}: hard={} geometry_debt={} bad_source={} bad_target={} endpoint_intrusions={} endpoint_reentries={} non_endpoint_hits={}",
                metrics.hard_violation_count(),
                metrics.geometry_debt_count(),
                metrics.bad_source_exits,
                metrics.bad_target_entries,
                metrics.endpoint_node_intrusions,
                metrics.endpoint_node_reentries,
                metrics.non_endpoint_node_hits
            ));
        }
    }

    assert!(checked > 0, "expected at least one flowchart fixture");
    assert!(
        failures.is_empty(),
        "flowchart routing quality failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
