use std::path::{Path, PathBuf};

use mermaid_rs_renderer::layout::{FlowchartQualityMetrics, flowchart_quality_metrics};
use mermaid_rs_renderer::{LayoutConfig, Theme, compute_layout, parse_mermaid, render_svg};

const DEFAULT_FIXTURES: &[&str] = &[
    "tests/fixtures/flowchart/ports_arrow_pathing_regression.mmd",
    "tests/fixtures/flowchart/complex.mmd",
    "tests/fixtures/flowchart/cycles.mmd",
    "tests/fixtures/flowchart/subgraph_direction.mmd",
    "benches/fixtures/flowchart_ports_heavy.mmd",
    "benches/fixtures/flowchart_long_edge_labels.mmd",
    "benches/fixtures/flowchart_path_occlusion_maze.mmd",
];

struct RenderedFixture {
    title: String,
    source_path: PathBuf,
    svg_file: String,
    source: String,
    metrics: Option<FlowchartQualityMetrics>,
}

fn html_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            '\'' => "&#39;".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

fn sanitize_file_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("flowchart");
    let sanitized = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "flowchart".to_string()
    } else {
        sanitized
    }
}

fn render_fixture(path: &Path, output_dir: &Path) -> anyhow::Result<RenderedFixture> {
    let source = std::fs::read_to_string(path)?;
    let parsed = parse_mermaid(&source)?;
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);
    let metrics = flowchart_quality_metrics(&layout);

    let svg_file = format!("{}.svg", sanitize_file_stem(path));
    std::fs::write(output_dir.join(&svg_file), svg)?;
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("flowchart")
        .replace('_', " ");

    Ok(RenderedFixture {
        title,
        source_path: path.to_path_buf(),
        svg_file,
        source,
        metrics,
    })
}

fn metrics_html(metrics: Option<FlowchartQualityMetrics>) -> String {
    let Some(metrics) = metrics else {
        return "<p class=\"metrics bad\">No flowchart metrics</p>".to_string();
    };
    let status = if metrics.geometry_debt_count() == 0 {
        "good"
    } else {
        "bad"
    };
    format!(
        "<dl class=\"metrics {status}\">\
         <dt>hard</dt><dd>{hard}</dd>\
         <dt>geometry debt</dt><dd>{debt}</dd>\
         <dt>edges</dt><dd>{edges}</dd>\
         <dt>bends/edge</dt><dd>{bends:.2}</dd>\
         <dt>path/manhattan</dt><dd>{ratio:.2}</dd>\
         <dt>crossings</dt><dd>{crossings}</dd>\
         </dl>",
        hard = metrics.hard_violation_count(),
        debt = metrics.geometry_debt_count(),
        edges = metrics.edge_count,
        bends = metrics.bends as f32 / metrics.edge_count.max(1) as f32,
        ratio = metrics.path_to_center_manhattan_ratio,
        crossings = metrics.crossings,
    )
}

fn write_report(output_dir: &Path, fixtures: &[RenderedFixture]) -> anyhow::Result<PathBuf> {
    let mut html = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Flowchart visual routing report</title>\
         <style>\
         body{font-family:Inter,system-ui,sans-serif;margin:24px;background:#f6f7fb;color:#1f2937}\
         h1{margin-bottom:4px}.note{color:#596273;margin-top:0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(520px,1fr));gap:18px}\
         .card{background:#fff;border:1px solid #d8dee9;border-radius:12px;padding:16px;box-shadow:0 1px 4px #0001}\
         object{width:100%;min-height:360px;border:1px solid #e5e7eb;background:white}.metrics{display:grid;grid-template-columns:repeat(6,auto);gap:4px 12px;align-items:baseline}\
         .metrics dt{font-size:12px;text-transform:uppercase;color:#6b7280}.metrics dd{margin:0;font-weight:700}.good dd:first-of-type,.good dd:nth-of-type(2){color:#047857}.bad dd:first-of-type,.bad dd:nth-of-type(2){color:#b91c1c}\
         details{margin-top:10px}pre{overflow:auto;background:#111827;color:#e5e7eb;border-radius:8px;padding:12px;font-size:12px}\
         </style><h1>Flowchart visual routing report</h1>\
         <p class=\"note\">Fixtures target arrowheads, port selection, self-loops, labels, dense hubs, and subgraph boundary crossings. Hard violations and geometry debt should remain zero.</p><div class=\"grid\">",
    );

    for fixture in fixtures {
        html.push_str(&format!(
            "<section class=\"card\"><h2>{}</h2><p class=\"note\">{}</p>{}<object type=\"image/svg+xml\" data=\"{}\"></object><details><summary>Mermaid source</summary><pre>{}</pre></details></section>",
            html_escape(&fixture.title),
            html_escape(&fixture.source_path.display().to_string()),
            metrics_html(fixture.metrics),
            html_escape(&fixture.svg_file),
            html_escape(&fixture.source),
        ));
    }
    html.push_str("</div>");

    let report_path = output_dir.join("index.html");
    std::fs::write(&report_path, html)?;
    Ok(report_path)
}

fn parse_args() -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let mut output_dir = PathBuf::from("tmp/flowchart-visual-report");
    let mut fixtures = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a directory"))?;
                output_dir = PathBuf::from(value);
            }
            "--help" | "-h" => {
                println!("Usage: flowchart_visual_report [--output DIR] [FIXTURE.mmd ...]");
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => anyhow::bail!("unknown option {arg}"),
            _ => fixtures.push(PathBuf::from(arg)),
        }
    }
    if fixtures.is_empty() {
        fixtures = DEFAULT_FIXTURES.iter().map(PathBuf::from).collect();
    }
    Ok((output_dir, fixtures))
}

fn main() -> anyhow::Result<()> {
    let (output_dir, fixture_paths) = parse_args()?;
    std::fs::create_dir_all(&output_dir)?;
    let mut fixtures = Vec::with_capacity(fixture_paths.len());
    for path in fixture_paths {
        fixtures.push(render_fixture(&path, &output_dir)?);
    }
    let report_path = write_report(&output_dir, &fixtures)?;
    println!("{}", report_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(html_escape("A<&>\"'"), "A&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn sanitizes_svg_file_stems() {
        assert_eq!(sanitize_file_stem(Path::new("a b/c.d.mmd")), "c_d");
    }
}
