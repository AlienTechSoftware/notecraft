use criterion::{BenchmarkId, Criterion, criterion_group};
use mermaid_rs_renderer::config::LayoutConfig;
use mermaid_rs_renderer::layout::compute_layout;
use mermaid_rs_renderer::parser::parse_mermaid;
use mermaid_rs_renderer::render::render_svg;
use mermaid_rs_renderer::theme::Theme;
use std::hint::black_box;
use std::time::Duration;

const BENCH_FIXTURES: &[&str] = &[
    "flowchart_small",
    "flowchart_medium",
    "flowchart_large",
    "flowchart_tiny",
    "flowchart_ports_heavy",
    "flowchart_weave",
    "flowchart_backedges_subgraphs",
    "flowchart_sparse_components",
    "flowchart_lanes_crossfeed",
    "flowchart_grid_feedback",
    "flowchart_fanout_returns",
    "flowchart_label_collision",
    "flowchart_nested_clusters",
    "flowchart_asymmetric_components",
    "flowchart_parallel_merges",
    "flowchart_long_edge_labels",
    "flowchart_selfloop_bidi",
    "flowchart_component_packing",
    "flowchart_direction_conflict",
    "flowchart_parallel_label_stack",
    "flowchart_port_alignment_matrix",
    "flowchart_path_occlusion_maze",
    "flowchart_subgraph_boundary_intrusion",
    "flowchart_parallel_edges_bundle",
    "flowchart_flow_direction_backtrack",
    "flowchart_mega_multihub_control",
    "flowchart_mega_crosslane_subgraphs",
    "flowchart_mega_braid_feedback",
    "flowchart_mega_event_mesh",
    "flowchart_mega_nested_regions",
    "class_tiny",
    "state_tiny",
    "sequence_tiny",
    "class_medium",
    "state_medium",
    "sequence_medium",
    "er_medium",
    "pie_medium",
    "mindmap_medium",
    "journey_medium",
    "timeline_medium",
    "gantt_medium",
    "requirement_medium",
    "gitgraph_medium",
    "c4_medium",
    "sankey_medium",
    "quadrant_medium",
    "zenuml_medium",
    "block_medium",
    "packet_medium",
    "kanban_medium",
    "architecture_medium",
    "radar_medium",
    "treemap_medium",
    "xychart_medium",
];

fn dense_flowchart_source(nodes: usize, extra_edges: usize) -> String {
    let mut out = String::from("flowchart LR\n");
    if nodes == 0 {
        return out;
    }
    for i in 0..nodes {
        out.push_str(&format!("  N{}[Node {}]\n", i, i));
    }
    for i in 0..nodes.saturating_sub(1) {
        out.push_str(&format!("  N{} --> N{}\n", i, i + 1));
    }
    let mut count = 0usize;
    for i in 0..nodes {
        for j in (i + 2)..nodes {
            if count >= extra_edges {
                break;
            }
            out.push_str(&format!("  N{} --> N{}\n", i, j));
            count += 1;
        }
        if count >= extra_edges {
            break;
        }
    }
    out
}

fn fixture(name: &str) -> &'static str {
    match name {
        "flowchart_small" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_small.mmd"
        )),
        "flowchart_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_medium.mmd"
        )),
        "flowchart_large" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_large.mmd"
        )),
        "flowchart_tiny" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_tiny.mmd"
        )),
        "flowchart_ports_heavy" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_ports_heavy.mmd"
        )),
        "flowchart_weave" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_weave.mmd"
        )),
        "flowchart_backedges_subgraphs" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_backedges_subgraphs.mmd"
        )),
        "flowchart_sparse_components" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_sparse_components.mmd"
        )),
        "flowchart_lanes_crossfeed" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_lanes_crossfeed.mmd"
        )),
        "flowchart_grid_feedback" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_grid_feedback.mmd"
        )),
        "flowchart_fanout_returns" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_fanout_returns.mmd"
        )),
        "flowchart_label_collision" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_label_collision.mmd"
        )),
        "flowchart_nested_clusters" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_nested_clusters.mmd"
        )),
        "flowchart_asymmetric_components" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_asymmetric_components.mmd"
        )),
        "flowchart_parallel_merges" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_parallel_merges.mmd"
        )),
        "flowchart_long_edge_labels" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_long_edge_labels.mmd"
        )),
        "flowchart_selfloop_bidi" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_selfloop_bidi.mmd"
        )),
        "flowchart_component_packing" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_component_packing.mmd"
        )),
        "flowchart_direction_conflict" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_direction_conflict.mmd"
        )),
        "flowchart_parallel_label_stack" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_parallel_label_stack.mmd"
        )),
        "flowchart_port_alignment_matrix" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_port_alignment_matrix.mmd"
        )),
        "flowchart_path_occlusion_maze" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_path_occlusion_maze.mmd"
        )),
        "flowchart_subgraph_boundary_intrusion" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_subgraph_boundary_intrusion.mmd"
        )),
        "flowchart_parallel_edges_bundle" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_parallel_edges_bundle.mmd"
        )),
        "flowchart_flow_direction_backtrack" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_flow_direction_backtrack.mmd"
        )),
        "flowchart_mega_multihub_control" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_mega_multihub_control.mmd"
        )),
        "flowchart_mega_crosslane_subgraphs" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_mega_crosslane_subgraphs.mmd"
        )),
        "flowchart_mega_braid_feedback" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_mega_braid_feedback.mmd"
        )),
        "flowchart_mega_event_mesh" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_mega_event_mesh.mmd"
        )),
        "flowchart_mega_nested_regions" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/flowchart_mega_nested_regions.mmd"
        )),
        "class_tiny" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/class_tiny.mmd"
        )),
        "state_tiny" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/state_tiny.mmd"
        )),
        "sequence_tiny" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/sequence_tiny.mmd"
        )),
        "class_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/class_medium.mmd"
        )),
        "state_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/state_medium.mmd"
        )),
        "sequence_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/sequence_medium.mmd"
        )),
        "er_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/er_medium.mmd"
        )),
        "pie_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/pie_medium.mmd"
        )),
        "mindmap_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/mindmap_medium.mmd"
        )),
        "journey_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/journey_medium.mmd"
        )),
        "timeline_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/timeline_medium.mmd"
        )),
        "gantt_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/gantt_medium.mmd"
        )),
        "requirement_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/requirement_medium.mmd"
        )),
        "gitgraph_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/gitgraph_medium.mmd"
        )),
        "c4_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/c4_medium.mmd"
        )),
        "sankey_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/sankey_medium.mmd"
        )),
        "quadrant_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/quadrant_medium.mmd"
        )),
        "zenuml_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/zenuml_medium.mmd"
        )),
        "block_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/block_medium.mmd"
        )),
        "packet_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/packet_medium.mmd"
        )),
        "kanban_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/kanban_medium.mmd"
        )),
        "architecture_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/architecture_medium.mmd"
        )),
        "radar_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/radar_medium.mmd"
        )),
        "treemap_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/treemap_medium.mmd"
        )),
        "xychart_medium" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches/fixtures/xychart_medium.mmd"
        )),
        _ => panic!("unknown fixture"),
    }
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for name in [
        "flowchart_small",
        "flowchart_medium",
        "flowchart_large",
        "flowchart_tiny",
        "flowchart_ports_heavy",
        "flowchart_weave",
        "flowchart_backedges_subgraphs",
        "flowchart_sparse_components",
        "flowchart_lanes_crossfeed",
        "flowchart_grid_feedback",
        "flowchart_fanout_returns",
        "flowchart_label_collision",
        "flowchart_nested_clusters",
        "flowchart_asymmetric_components",
        "flowchart_parallel_merges",
        "flowchart_long_edge_labels",
        "flowchart_selfloop_bidi",
        "flowchart_component_packing",
        "flowchart_direction_conflict",
        "flowchart_parallel_label_stack",
        "flowchart_port_alignment_matrix",
        "flowchart_path_occlusion_maze",
        "flowchart_subgraph_boundary_intrusion",
        "flowchart_parallel_edges_bundle",
        "flowchart_flow_direction_backtrack",
        "flowchart_mega_multihub_control",
        "flowchart_mega_crosslane_subgraphs",
        "flowchart_mega_braid_feedback",
        "flowchart_mega_event_mesh",
        "flowchart_mega_nested_regions",
        "class_tiny",
        "state_tiny",
        "sequence_tiny",
        "class_medium",
        "state_medium",
        "sequence_medium",
        "er_medium",
        "pie_medium",
        "mindmap_medium",
        "journey_medium",
        "timeline_medium",
        "gantt_medium",
        "requirement_medium",
        "gitgraph_medium",
        "c4_medium",
        "sankey_medium",
        "quadrant_medium",
        "zenuml_medium",
        "block_medium",
        "packet_medium",
        "kanban_medium",
        "architecture_medium",
        "radar_medium",
        "treemap_medium",
        "xychart_medium",
    ] {
        let input = fixture(name);
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, data| {
            b.iter(|| {
                let parsed = parse_mermaid(black_box(data)).expect("parse failed");
                black_box(parsed.graph.nodes.len());
            });
        });
    }
    group.finish();
}

fn bench_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout");
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    for name in [
        "flowchart_tiny",
        "flowchart_medium",
        "flowchart_large",
        "flowchart_ports_heavy",
        "flowchart_weave",
        "flowchart_backedges_subgraphs",
        "flowchart_sparse_components",
        "flowchart_lanes_crossfeed",
        "flowchart_grid_feedback",
        "flowchart_fanout_returns",
        "flowchart_label_collision",
        "flowchart_nested_clusters",
        "flowchart_asymmetric_components",
        "flowchart_parallel_merges",
        "flowchart_long_edge_labels",
        "flowchart_selfloop_bidi",
        "flowchart_component_packing",
        "flowchart_direction_conflict",
        "flowchart_parallel_label_stack",
        "flowchart_port_alignment_matrix",
        "flowchart_path_occlusion_maze",
        "flowchart_subgraph_boundary_intrusion",
        "flowchart_parallel_edges_bundle",
        "flowchart_flow_direction_backtrack",
        "flowchart_mega_multihub_control",
        "flowchart_mega_crosslane_subgraphs",
        "flowchart_mega_braid_feedback",
        "flowchart_mega_event_mesh",
        "flowchart_mega_nested_regions",
        "class_tiny",
        "class_medium",
        "state_tiny",
        "state_medium",
        "sequence_tiny",
        "sequence_medium",
        "er_medium",
        "pie_medium",
        "mindmap_medium",
        "journey_medium",
        "timeline_medium",
        "gantt_medium",
        "requirement_medium",
        "gitgraph_medium",
        "c4_medium",
        "sankey_medium",
        "quadrant_medium",
        "zenuml_medium",
        "block_medium",
        "packet_medium",
        "kanban_medium",
        "architecture_medium",
        "radar_medium",
        "treemap_medium",
        "xychart_medium",
    ] {
        let parsed = parse_mermaid(fixture(name)).expect("parse failed");
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &parsed.graph,
            |b, graph| {
                b.iter(|| {
                    let layout = compute_layout(black_box(graph), &theme, &config);
                    black_box(layout.nodes.len());
                });
            },
        );
    }
    group.finish();
}

fn bench_edge_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_edge_routing");
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    for (nodes, extra_edges) in [(40usize, 80usize), (60, 180), (80, 320)] {
        let name = format!("dense_{}_{}", nodes, extra_edges);
        let input = dense_flowchart_source(nodes, extra_edges);
        let parsed = parse_mermaid(&input).expect("parse failed");
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &parsed.graph,
            |b, graph| {
                b.iter(|| {
                    let layout = compute_layout(black_box(graph), &theme, &config);
                    black_box(layout.edges.len());
                });
            },
        );
    }
    group.finish();
}

fn bench_edge_routing_grid_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_edge_routing_grid_modes");
    let theme = Theme::modern();
    let mut config_grid = LayoutConfig::default();
    config_grid.flowchart.routing.enable_grid_router = true;
    let mut config_heur = LayoutConfig::default();
    config_heur.flowchart.routing.enable_grid_router = false;

    for (nodes, extra_edges) in [(40usize, 80usize), (60, 180), (80, 320)] {
        let name = format!("dense_{}_{}", nodes, extra_edges);
        let input = dense_flowchart_source(nodes, extra_edges);
        let parsed = parse_mermaid(&input).expect("parse failed");
        group.bench_with_input(
            BenchmarkId::new("grid", &name),
            &parsed.graph,
            |b, graph| {
                b.iter(|| {
                    let layout = compute_layout(black_box(graph), &theme, &config_grid);
                    black_box(layout.edges.len());
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("heuristic", &name),
            &parsed.graph,
            |b, graph| {
                b.iter(|| {
                    let layout = compute_layout(black_box(graph), &theme, &config_heur);
                    black_box(layout.edges.len());
                });
            },
        );
    }
    group.finish();
}

fn bench_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_svg");
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    for name in [
        "flowchart_tiny",
        "flowchart_medium",
        "flowchart_large",
        "flowchart_ports_heavy",
        "flowchart_weave",
        "flowchart_backedges_subgraphs",
        "flowchart_sparse_components",
        "flowchart_lanes_crossfeed",
        "flowchart_grid_feedback",
        "flowchart_fanout_returns",
        "flowchart_label_collision",
        "flowchart_nested_clusters",
        "flowchart_asymmetric_components",
        "flowchart_parallel_merges",
        "flowchart_long_edge_labels",
        "flowchart_selfloop_bidi",
        "flowchart_component_packing",
        "flowchart_direction_conflict",
        "flowchart_parallel_label_stack",
        "flowchart_port_alignment_matrix",
        "flowchart_path_occlusion_maze",
        "flowchart_subgraph_boundary_intrusion",
        "flowchart_parallel_edges_bundle",
        "flowchart_flow_direction_backtrack",
        "flowchart_mega_multihub_control",
        "flowchart_mega_crosslane_subgraphs",
        "flowchart_mega_braid_feedback",
        "flowchart_mega_event_mesh",
        "flowchart_mega_nested_regions",
        "class_tiny",
        "class_medium",
        "state_tiny",
        "state_medium",
        "sequence_tiny",
        "sequence_medium",
        "er_medium",
        "pie_medium",
        "mindmap_medium",
        "journey_medium",
        "timeline_medium",
        "gantt_medium",
        "requirement_medium",
        "gitgraph_medium",
        "c4_medium",
        "sankey_medium",
        "quadrant_medium",
        "zenuml_medium",
        "block_medium",
        "packet_medium",
        "kanban_medium",
        "architecture_medium",
        "radar_medium",
        "treemap_medium",
        "xychart_medium",
    ] {
        let parsed = parse_mermaid(fixture(name)).expect("parse failed");
        let layout = compute_layout(&parsed.graph, &theme, &config);
        group.bench_with_input(BenchmarkId::from_parameter(name), &layout, |b, data| {
            b.iter(|| {
                let svg = render_svg(black_box(data), &theme, &config);
                black_box(svg.len());
            });
        });
    }
    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    for name in [
        "flowchart_tiny",
        "flowchart_small",
        "flowchart_medium",
        "flowchart_ports_heavy",
        "flowchart_weave",
        "flowchart_backedges_subgraphs",
        "flowchart_sparse_components",
        "flowchart_lanes_crossfeed",
        "flowchart_grid_feedback",
        "flowchart_fanout_returns",
        "flowchart_label_collision",
        "flowchart_nested_clusters",
        "flowchart_asymmetric_components",
        "flowchart_parallel_merges",
        "flowchart_long_edge_labels",
        "flowchart_selfloop_bidi",
        "flowchart_component_packing",
        "flowchart_direction_conflict",
        "flowchart_parallel_label_stack",
        "flowchart_port_alignment_matrix",
        "flowchart_path_occlusion_maze",
        "flowchart_subgraph_boundary_intrusion",
        "flowchart_parallel_edges_bundle",
        "flowchart_flow_direction_backtrack",
        "flowchart_mega_multihub_control",
        "flowchart_mega_crosslane_subgraphs",
        "flowchart_mega_braid_feedback",
        "flowchart_mega_event_mesh",
        "flowchart_mega_nested_regions",
        "class_tiny",
        "class_medium",
        "state_tiny",
        "state_medium",
        "sequence_tiny",
        "sequence_medium",
        "er_medium",
        "pie_medium",
        "mindmap_medium",
        "journey_medium",
        "timeline_medium",
        "gantt_medium",
        "requirement_medium",
        "gitgraph_medium",
        "c4_medium",
        "sankey_medium",
        "quadrant_medium",
        "zenuml_medium",
        "block_medium",
        "packet_medium",
        "kanban_medium",
        "architecture_medium",
        "radar_medium",
        "treemap_medium",
        "xychart_medium",
    ] {
        let input = fixture(name);
        group.bench_with_input(BenchmarkId::from_parameter(name), input, |b, data| {
            b.iter(|| {
                let parsed = parse_mermaid(black_box(data)).expect("parse failed");
                let layout = compute_layout(&parsed.graph, &theme, &config);
                let svg = render_svg(&layout, &theme, &config);
                black_box(svg.len());
            });
        });
    }
    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(100))
        .measurement_time(Duration::from_millis(300));
    targets = bench_parse, bench_layout, bench_edge_routing, bench_edge_routing_grid_modes, bench_render, bench_end_to_end
);

fn smoke_validate_bench_inputs() {
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    for name in BENCH_FIXTURES {
        parse_mermaid(fixture(name)).expect("parse failed");
    }

    for name in [
        "flowchart_tiny",
        "flowchart_long_edge_labels",
        "flowchart_label_collision",
        "class_tiny",
        "sequence_tiny",
        "pie_medium",
        "gantt_medium",
    ] {
        let parsed = parse_mermaid(fixture(name)).expect("parse failed");
        let layout = compute_layout(&parsed.graph, &theme, &config);
        let svg = render_svg(&layout, &theme, &config);
        assert!(
            layout.width.is_finite(),
            "{name}: layout width is not finite"
        );
        assert!(
            layout.height.is_finite(),
            "{name}: layout height is not finite"
        );
        assert!(!svg.contains("NaN"), "{name}: SVG contains NaN");
    }

    for (nodes, extra_edges) in [(40usize, 80usize), (60, 180)] {
        let input = dense_flowchart_source(nodes, extra_edges);
        let parsed = parse_mermaid(&input).expect("parse failed");
        let layout = compute_layout(&parsed.graph, &theme, &config);
        assert_eq!(layout.nodes.len(), nodes, "dense flowchart node count");
    }
}

fn main() {
    if std::env::var_os("MMDR_RUN_CRITERION_BENCHES").is_some() {
        benches();
        Criterion::default().configure_from_args().final_summary();
    } else {
        smoke_validate_bench_inputs();
    }
}
