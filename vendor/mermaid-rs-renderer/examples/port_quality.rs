use std::path::{Path, PathBuf};

use mermaid_rs_renderer::layout::{
    EdgeLayout, FlowchartQualityMetrics, NodeLayout, flowchart_quality_metrics,
};
use mermaid_rs_renderer::{
    DiagramKind, LayoutConfig, NodeShape, Theme, compute_layout, parse_mermaid,
};
use serde::Serialize;

#[derive(Default, Serialize)]
struct Totals {
    files: usize,
    flowcharts: usize,
    parse_failures: usize,
    edges: usize,
    bad_source_exits: usize,
    bad_target_entries: usize,
    endpoint_node_intrusions: usize,
    endpoint_node_reentries: usize,
    non_endpoint_node_hits: usize,
    crossings: usize,
    bends: usize,
    path_len: f32,
    center_manhattan: f32,
    quality_score: f32,
}

impl Totals {
    fn hard_violation_count(&self) -> usize {
        self.bad_source_exits
            + self.bad_target_entries
            + self.endpoint_node_intrusions
            + self.non_endpoint_node_hits
    }

    fn geometry_debt_count(&self) -> usize {
        self.hard_violation_count() + self.endpoint_node_reentries
    }

    fn edge_count_for_average(&self) -> f32 {
        self.edges.max(1) as f32
    }

    fn avg_bends_per_edge(&self) -> f32 {
        self.bends as f32 / self.edge_count_for_average()
    }

    fn avg_path_to_center_manhattan_ratio(&self) -> f32 {
        self.path_len / self.center_manhattan.max(1.0)
    }

    fn add_quality_metrics(&mut self, metrics: FlowchartQualityMetrics) {
        self.edges += metrics.edge_count;
        self.bad_source_exits += metrics.bad_source_exits;
        self.bad_target_entries += metrics.bad_target_entries;
        self.endpoint_node_intrusions += metrics.endpoint_node_intrusions;
        self.endpoint_node_reentries += metrics.endpoint_node_reentries;
        self.non_endpoint_node_hits += metrics.non_endpoint_node_hits;
        self.crossings += metrics.crossings;
        self.bends += metrics.bends;
        self.path_len += metrics.path_length;
        self.center_manhattan += metrics.center_manhattan;
        self.quality_score += metrics.quality_score;
    }
}

#[derive(Debug, Clone)]
struct Options {
    root: String,
    json: bool,
    verbose: bool,
    strict: bool,
    max_hard_violations: Option<usize>,
    max_geometry_debt: Option<usize>,
    max_avg_bends: Option<f32>,
    max_path_ratio: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: "benches/fixtures".to_string(),
            json: false,
            verbose: false,
            strict: false,
            max_hard_violations: None,
            max_geometry_debt: None,
            max_avg_bends: None,
            max_path_ratio: None,
        }
    }
}

#[derive(Serialize)]
struct QualityReport<'a> {
    root: &'a str,
    totals: &'a Totals,
    hard_violations: usize,
    geometry_debt: usize,
    avg_bends_per_edge: f32,
    avg_path_to_center_manhattan_ratio: f32,
    failed_thresholds: &'a [String],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn collect_mmd(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_mmd(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "mmd") {
                out.push(path);
            }
        }
    }
}

fn side_for_point(node: &NodeLayout, p: (f32, f32)) -> Side {
    let left = (p.0 - node.x).abs();
    let right = (p.0 - (node.x + node.width)).abs();
    let top = (p.1 - node.y).abs();
    let bottom = (p.1 - (node.y + node.height)).abs();
    let mut best = (left, Side::Left);
    for candidate in [
        (right, Side::Right),
        (top, Side::Top),
        (bottom, Side::Bottom),
    ] {
        if candidate.0 < best.0 {
            best = candidate;
        }
    }
    best.1
}

fn source_exits_outward(side: Side, start: (f32, f32), next: (f32, f32)) -> bool {
    let eps = 0.5;
    match side {
        Side::Left => next.0 <= start.0 + eps,
        Side::Right => next.0 >= start.0 - eps,
        Side::Top => next.1 <= start.1 + eps,
        Side::Bottom => next.1 >= start.1 - eps,
    }
}

fn target_enters_from_outside(side: Side, prev: (f32, f32), end: (f32, f32)) -> bool {
    let eps = 0.5;
    match side {
        Side::Left => prev.0 <= end.0 + eps,
        Side::Right => prev.0 >= end.0 - eps,
        Side::Top => prev.1 <= end.1 + eps,
        Side::Bottom => prev.1 >= end.1 - eps,
    }
}

fn segment_intrudes_endpoint_rect(
    side: Side,
    outside: (f32, f32),
    endpoint: (f32, f32),
    node: &NodeLayout,
) -> bool {
    // Detect the common visual bug: final/initial segment reaches a side from the opposite side,
    // so the arrow shaft travels through the node interior before touching the declared port.
    let eps = 0.5;
    let within_y = endpoint.1 >= node.y - eps && endpoint.1 <= node.y + node.height + eps;
    let within_x = endpoint.0 >= node.x - eps && endpoint.0 <= node.x + node.width + eps;
    match side {
        Side::Left => within_y && outside.0 > endpoint.0 + eps,
        Side::Right => within_y && outside.0 < endpoint.0 - eps,
        Side::Top => within_x && outside.1 > endpoint.1 + eps,
        Side::Bottom => within_x && outside.1 < endpoint.1 - eps,
    }
}

fn rect_contains_strict(node: &NodeLayout, p: (f32, f32)) -> bool {
    let eps = 0.5;
    match node.shape {
        NodeShape::Diamond => {
            let cx = node.x + node.width * 0.5;
            let cy = node.y + node.height * 0.5;
            let rx = (node.width * 0.5 - eps).max(1.0);
            let ry = (node.height * 0.5 - eps).max(1.0);
            (p.0 - cx).abs() / rx + (p.1 - cy).abs() / ry < 1.0
        }
        NodeShape::Circle | NodeShape::DoubleCircle => {
            let cx = node.x + node.width * 0.5;
            let cy = node.y + node.height * 0.5;
            let rx = (node.width * 0.5 - eps).max(1.0);
            let ry = (node.height * 0.5 - eps).max(1.0);
            let nx = (p.0 - cx) / rx;
            let ny = (p.1 - cy) / ry;
            nx * nx + ny * ny < 1.0
        }
        _ => {
            p.0 > node.x + eps
                && p.0 < node.x + node.width - eps
                && p.1 > node.y + eps
                && p.1 < node.y + node.height - eps
        }
    }
}

fn segment_hits_rect_interior(a: (f32, f32), b: (f32, f32), node: &NodeLayout) -> bool {
    let steps = (((b.0 - a.0).hypot(b.1 - a.1) / 4.0).ceil() as usize).max(1);
    (1..steps).any(|i| {
        let t = i as f32 / steps as f32;
        rect_contains_strict(node, (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
    })
}

fn endpoint_reentry_count(points: &[(f32, f32)], node: &NodeLayout, is_source: bool) -> usize {
    if points.len() < 3 {
        return 0;
    }
    let last_segment_idx = points.len().saturating_sub(2);
    points
        .windows(2)
        .enumerate()
        .filter(|(idx, segment)| {
            let allowed_endpoint_stub = if is_source {
                *idx == 0
            } else {
                *idx == last_segment_idx
            };
            !allowed_endpoint_stub && segment_hits_rect_interior(segment[0], segment[1], node)
        })
        .count()
}

fn path_len(points: &[(f32, f32)]) -> f32 {
    points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

fn bend_count(points: &[(f32, f32)]) -> usize {
    points
        .windows(3)
        .filter(|w| {
            let a = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            let b = (w[2].0 - w[1].0, w[2].1 - w[1].1);
            (a.0 * b.1 - a.1 * b.0).abs() > 1e-3
        })
        .count()
}

fn score_edge(
    file: &Path,
    edge_idx: usize,
    edge: &EdgeLayout,
    layout_nodes: &std::collections::BTreeMap<String, NodeLayout>,
    totals: &mut Totals,
    verbose: bool,
) {
    if edge.points.len() < 2 {
        return;
    }
    let Some(from) = layout_nodes.get(&edge.from) else {
        return;
    };
    let Some(to) = layout_nodes.get(&edge.to) else {
        return;
    };
    let start = edge.points[0];
    let end = *edge.points.last().unwrap();
    let start_side = side_for_point(from, start);
    let end_side = side_for_point(to, end);
    let next = edge.points[1];
    let prev = edge.points[edge.points.len() - 2];

    totals.edges += 1;
    if !source_exits_outward(start_side, start, next) {
        totals.bad_source_exits += 1;
        if verbose {
            eprintln!(
                "{} edge#{edge_idx} {}->{} bad source exit {:?}",
                file.display(),
                edge.from,
                edge.to,
                edge.points
            );
        }
    }
    if !target_enters_from_outside(end_side, prev, end) {
        totals.bad_target_entries += 1;
        if verbose {
            eprintln!(
                "{} edge#{edge_idx} {}->{} bad target entry {:?}",
                file.display(),
                edge.from,
                edge.to,
                edge.points
            );
        }
    }
    if segment_intrudes_endpoint_rect(start_side, next, start, from) {
        totals.endpoint_node_intrusions += 1;
    }
    if segment_intrudes_endpoint_rect(end_side, prev, end, to) {
        totals.endpoint_node_intrusions += 1;
    }
    let source_reentries = endpoint_reentry_count(&edge.points, from, true);
    let target_reentries = if edge.to == edge.from {
        0
    } else {
        endpoint_reentry_count(&edge.points, to, false)
    };
    totals.endpoint_node_reentries += source_reentries;
    totals.endpoint_node_reentries += target_reentries;
    if verbose && source_reentries + target_reentries > 0 {
        eprintln!(
            "{} edge#{edge_idx} {}->{} endpoint reentries source={} target={} {:?}",
            file.display(),
            edge.from,
            edge.to,
            source_reentries,
            target_reentries,
            edge.points
        );
    }

    for (id, node) in layout_nodes {
        if id == &edge.from || id == &edge.to || node.hidden || node.anchor_subgraph.is_some() {
            continue;
        }
        if edge
            .points
            .windows(2)
            .any(|w| segment_hits_rect_interior(w[0], w[1], node))
        {
            totals.non_endpoint_node_hits += 1;
            if verbose {
                eprintln!(
                    "{} edge#{edge_idx} {}->{} hits non-endpoint node {} {:?}",
                    file.display(),
                    edge.from,
                    edge.to,
                    id,
                    edge.points
                );
            }
        }
    }

    totals.bends += bend_count(&edge.points);
    totals.path_len += path_len(&edge.points);
    let from_c = (from.x + from.width / 2.0, from.y + from.height / 2.0);
    let to_c = (to.x + to.width / 2.0, to.y + to.height / 2.0);
    totals.center_manhattan += (to_c.0 - from_c.0).abs() + (to_c.1 - from_c.1).abs();
}

fn parse_options_from<I>(args: I) -> anyhow::Result<Options>
where
    I: IntoIterator<Item = String>,
{
    let mut options = Options::default();
    let mut args = args.into_iter();
    let _program = args.next();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--strict" => options.strict = true,
            "--verbose" | "-v" => options.verbose = true,
            "--max-hard" | "--max-hard-violations" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                options.max_hard_violations = Some(value.parse()?);
            }
            "--max-geometry-debt" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                options.max_geometry_debt = Some(value.parse()?);
            }
            "--max-bends" | "--max-avg-bends" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                options.max_avg_bends = Some(value.parse()?);
            }
            "--max-path-ratio" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                options.max_path_ratio = Some(value.parse()?);
            }
            "--help" | "-h" => {
                println!(
                    "Usage: port_quality [OPTIONS] [ROOT]\n\n\
                     Options:\n\
                       --json                         Print a JSON report\n\
                       --strict                       Fail unless hard violations and geometry debt are zero\n\
                       --max-hard N                   Fail if hard violations exceed N\n\
                       --max-geometry-debt N          Fail if hard violations plus endpoint reentries exceed N\n\
                       --max-bends N                  Fail if average bends per edge exceeds N\n\
                       --max-path-ratio N             Fail if path/manhattan ratio exceeds N\n\
                       --verbose, -v                  Print edge-level diagnostics"
                );
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => anyhow::bail!("unknown option {arg}"),
            _ => options.root = arg,
        }
    }
    Ok(options)
}

fn threshold_failures(totals: &Totals, options: &Options) -> Vec<String> {
    let hard_limit = options
        .max_hard_violations
        .or_else(|| options.strict.then_some(0));
    let geometry_limit = options
        .max_geometry_debt
        .or_else(|| options.strict.then_some(0));
    let mut failures = Vec::new();
    if let Some(limit) = hard_limit {
        let actual = totals.hard_violation_count();
        if actual > limit {
            failures.push(format!("hard_violations {actual} > {limit}"));
        }
    }
    if let Some(limit) = geometry_limit {
        let actual = totals.geometry_debt_count();
        if actual > limit {
            failures.push(format!("geometry_debt {actual} > {limit}"));
        }
    }
    if let Some(limit) = options.max_avg_bends {
        let actual = totals.avg_bends_per_edge();
        if actual > limit {
            failures.push(format!("avg_bends_per_edge {actual:.3} > {limit:.3}"));
        }
    }
    if let Some(limit) = options.max_path_ratio {
        let actual = totals.avg_path_to_center_manhattan_ratio();
        if actual > limit {
            failures.push(format!(
                "avg_path_to_center_manhattan_ratio {actual:.3} > {limit:.3}"
            ));
        }
    }
    failures
}

fn report<'a>(root: &'a str, totals: &'a Totals, failures: &'a [String]) -> QualityReport<'a> {
    QualityReport {
        root,
        totals,
        hard_violations: totals.hard_violation_count(),
        geometry_debt: totals.geometry_debt_count(),
        avg_bends_per_edge: totals.avg_bends_per_edge(),
        avg_path_to_center_manhattan_ratio: totals.avg_path_to_center_manhattan_ratio(),
        failed_thresholds: failures,
    }
}

fn print_text_report(totals: &Totals, failures: &[String]) {
    let edges = totals.edge_count_for_average();
    println!(
        "port_quality files={} flowcharts={} parse_failures={} edges={}",
        totals.files, totals.flowcharts, totals.parse_failures, totals.edges
    );
    println!(
        "hard_violations={} geometry_debt={}",
        totals.hard_violation_count(),
        totals.geometry_debt_count()
    );
    println!(
        "bad_source_exits={} ({:.2}%)",
        totals.bad_source_exits,
        totals.bad_source_exits as f32 * 100.0 / edges
    );
    println!(
        "bad_target_entries={} ({:.2}%)",
        totals.bad_target_entries,
        totals.bad_target_entries as f32 * 100.0 / edges
    );
    println!(
        "endpoint_node_intrusions={} ({:.2}% of endpoints)",
        totals.endpoint_node_intrusions,
        totals.endpoint_node_intrusions as f32 * 50.0 / edges
    );
    println!(
        "endpoint_node_reentries={} ({:.2}% per edge)",
        totals.endpoint_node_reentries,
        totals.endpoint_node_reentries as f32 * 100.0 / edges
    );
    println!(
        "non_endpoint_node_hits={} ({:.2}% per edge)",
        totals.non_endpoint_node_hits,
        totals.non_endpoint_node_hits as f32 * 100.0 / edges
    );
    println!("crossings={}", totals.crossings);
    println!("avg_bends_per_edge={:.2}", totals.avg_bends_per_edge());
    println!(
        "avg_path_to_center_manhattan_ratio={:.2}",
        totals.avg_path_to_center_manhattan_ratio()
    );
    println!("quality_score={:.2}", totals.quality_score);
    if !failures.is_empty() {
        println!("failed_thresholds={}", failures.join(", "));
    }
}

fn main() -> anyhow::Result<()> {
    let mut options = parse_options_from(std::env::args())?;
    options.verbose |= std::env::var_os("PORT_QUALITY_VERBOSE").is_some();

    let mut files = Vec::new();
    collect_mmd(Path::new(&options.root), &mut files);
    files.sort();

    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let mut totals = Totals::default();

    for file in files {
        totals.files += 1;
        let input = std::fs::read_to_string(&file)?;
        let parsed = match parse_mermaid(&input) {
            Ok(parsed) => parsed,
            Err(err) => {
                totals.parse_failures += 1;
                if options.verbose {
                    eprintln!("{} parse failed: {err}", file.display());
                }
                continue;
            }
        };
        if parsed.graph.kind != DiagramKind::Flowchart {
            continue;
        }
        totals.flowcharts += 1;
        let layout = compute_layout(&parsed.graph, &theme, &config);
        if let Some(metrics) = flowchart_quality_metrics(&layout) {
            if options.verbose {
                totals.crossings += metrics.crossings;
                totals.quality_score += metrics.quality_score;
                for (edge_idx, edge) in layout.edges.iter().enumerate() {
                    score_edge(
                        &file,
                        edge_idx,
                        edge,
                        &layout.nodes,
                        &mut totals,
                        options.verbose,
                    );
                }
            } else {
                totals.add_quality_metrics(metrics);
            }
        }
    }

    let failures = threshold_failures(&totals, &options);
    if options.json {
        let report = report(&options.root, &totals, &failures);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text_report(&totals, &failures);
    }

    if !failures.is_empty() {
        anyhow::bail!("port quality thresholds failed: {}", failures.join(", "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        parse_options_from(args.iter().map(|arg| (*arg).to_string())).expect("options parse")
    }

    #[test]
    fn parses_ci_friendly_threshold_options() {
        let options = parse(&[
            "port_quality",
            "--json",
            "--strict",
            "--max-hard",
            "0",
            "--max-bends",
            "3.5",
            "fixtures",
        ]);

        assert_eq!(options.root, "fixtures");
        assert!(options.json);
        assert!(options.strict);
        assert_eq!(options.max_hard_violations, Some(0));
        assert_eq!(options.max_avg_bends, Some(3.5));
    }

    #[test]
    fn strict_thresholds_fail_on_hard_or_geometry_debt() {
        let totals = Totals {
            edges: 2,
            bad_source_exits: 1,
            endpoint_node_reentries: 2,
            bends: 8,
            center_manhattan: 100.0,
            path_len: 180.0,
            ..Totals::default()
        };
        let options = Options {
            strict: true,
            max_avg_bends: Some(3.0),
            max_path_ratio: Some(1.5),
            ..Options::default()
        };

        let failures = threshold_failures(&totals, &options);

        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("hard_violations"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("geometry_debt"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("avg_bends_per_edge"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("avg_path_to_center_manhattan_ratio"))
        );
    }

    #[test]
    fn json_report_exposes_quality_totals_and_failures() {
        let totals = Totals {
            files: 1,
            flowcharts: 1,
            edges: 1,
            quality_score: 42.0,
            ..Totals::default()
        };
        let failures = vec!["hard_violations 1 > 0".to_string()];
        let json = serde_json::to_string(&report("fixtures", &totals, &failures))
            .expect("serialize report");

        assert!(json.contains("\"root\":\"fixtures\""));
        assert!(json.contains("\"hard_violations\":0"));
        assert!(json.contains("\"quality_score\":42.0"));
        assert!(json.contains("hard_violations 1 > 0"));
    }
}
