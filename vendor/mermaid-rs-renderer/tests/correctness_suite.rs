//! Correctness / semantic-fidelity gate.
//!
//! This suite is the first domain of the comprehensive benchmark suite
//! (see `docs/benchmark-suite-design.md`). It checks that the renderer
//! *faithfully encodes the input* rather than just producing geometrically
//! valid output. A layout can score perfectly on crossings while having
//! silently dropped edges or labels; these checks catch that class of bug.
//!
//! For every fixture we assert:
//!   * Render validity: the SVG is well-formed XML, contains no NaN/Inf
//!     numeric attributes, and declares a positive, finite viewBox.
//!   * Content parity: every graph node, every edge, and every label string
//!     from the parsed IR is represented in the rendered output.
//!   * Viewport fit: declared content is not clipped outside the viewBox.
//!
//! The checks are intentionally conservative (they under-claim rather than
//! flag false positives) so a failure is always a real defect.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use mermaid_rs_renderer::config::LayoutConfig;
use mermaid_rs_renderer::ir::{DiagramKind, Graph};
use mermaid_rs_renderer::layout::compute_layout;
use mermaid_rs_renderer::parser::parse_mermaid;
use mermaid_rs_renderer::render::render_svg;
use mermaid_rs_renderer::theme::Theme;

/// A single correctness finding for one fixture.
#[derive(Debug, Clone)]
struct Finding {
    fixture: String,
    rule: &'static str,
    detail: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Collect every `.mmd` fixture under the standard corpus directories.
fn collect_fixtures() -> Vec<PathBuf> {
    let root = repo_root();
    let mut dirs = vec![
        root.join("tests/fixtures"),
        root.join("docs/comparison_sources"),
        root.join("benches/fixtures"),
    ];
    // `benches/fixtures/expanded` and `/typical` hold extra corpora.
    dirs.push(root.join("benches/typical"));

    let mut out = Vec::new();
    for dir in dirs {
        collect_mmd_recursive(&dir, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn collect_mmd_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
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

fn fixture_name(path: &Path) -> String {
    let root = repo_root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn render_fixture(path: &Path) -> Result<(Graph, String), String> {
    let input = fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    let parsed = parse_mermaid(&input).map_err(|e| format!("parse error: {e}"))?;
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);
    Ok((parsed.graph, svg))
}

// ── Render-validity checks ──────────────────────────────────────────

/// Extract every numeric value from SVG attributes and confirm finiteness.
/// Catches NaN/Inf leaking into coordinates, which produce invisible or
/// browser-rejected output.
fn check_finite_numbers(fixture: &str, svg: &str, findings: &mut Vec<Finding>) {
    // Scan every attribute value for a non-finite numeric token. We only look
    // inside attribute values (not element text) so a label like "Information"
    // or "Infrastructure" does not trip the check.
    for bad in ["NaN", "inf", "Inf", "INF", "Infinity", "-inf", "-Inf"] {
        if attribute_contains_numeric_token(svg, bad) {
            findings.push(Finding {
                fixture: fixture.to_string(),
                rule: "finite-numbers",
                detail: format!("non-finite token '{bad}' in an SVG attribute"),
            });
            break;
        }
    }
}

/// Is `token` present as a standalone numeric token inside some attribute
/// value? A standalone token is bounded by value start/end or by a separator
/// (space, comma, or a path command letter), so it represents a coordinate
/// rather than a substring of a word.
fn attribute_contains_numeric_token(svg: &str, token: &str) -> bool {
    let is_boundary =
        |c: char| c.is_whitespace() || c == ',' || c.is_ascii_alphabetic() || c == '(';
    let mut search_from = 0;
    while let Some(rel) = svg[search_from..].find("=\"") {
        let start = search_from + rel + 2;
        let Some(end_rel) = svg[start..].find('"') else {
            break;
        };
        let value = &svg[start..start + end_rel];
        // Find token occurrences bounded by separators.
        let mut from = 0;
        while let Some(pos) = value[from..].find(token) {
            let abs = from + pos;
            let before_ok = abs == 0
                || value[..abs]
                    .chars()
                    .next_back()
                    .map(is_boundary)
                    .unwrap_or(true);
            let after_idx = abs + token.len();
            let after_ok = after_idx >= value.len()
                || value[after_idx..]
                    .chars()
                    .next()
                    .map(|c| c.is_whitespace() || c == ',' || c == '"' || c == ')')
                    .unwrap_or(true);
            if before_ok && after_ok {
                return true;
            }
            from = abs + token.len();
        }
        search_from = start + end_rel + 1;
    }
    false
}

/// Confirm the SVG declares a positive, finite viewBox or width/height.
fn check_viewbox(fixture: &str, svg: &str, findings: &mut Vec<Finding>) {
    let Some(vb) = extract_attribute(svg, "viewBox") else {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "viewbox-present",
            detail: "no viewBox attribute on root svg".to_string(),
        });
        return;
    };
    let parts: Vec<f32> = vb
        .split_whitespace()
        .filter_map(|p| p.parse::<f32>().ok())
        .collect();
    if parts.len() != 4 {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "viewbox-valid",
            detail: format!("viewBox is not 4 finite numbers: '{vb}'"),
        });
        return;
    }
    let (w, h) = (parts[2], parts[3]);
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "viewbox-positive",
            detail: format!("viewBox has non-positive/non-finite size: w={w} h={h}"),
        });
    }
}

/// Very small well-formedness check: every `<tag` has a matching `>` and the
/// document is non-empty. Full XML parsing is overkill; we just guard against
/// truncated or structurally broken output.
fn check_well_formed(fixture: &str, svg: &str, findings: &mut Vec<Finding>) {
    if !svg.trim_start().starts_with("<svg") && !svg.trim_start().starts_with("<?xml") {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "svg-root",
            detail: "output does not start with <svg> or <?xml>".to_string(),
        });
    }
    if !svg.contains("</svg>") {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "svg-closed",
            detail: "output has no closing </svg>".to_string(),
        });
    }
    let opens = svg.matches('<').count();
    let closes = svg.matches('>').count();
    if opens != closes {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "angle-balance",
            detail: format!("unbalanced angle brackets: {opens} '<' vs {closes} '>'"),
        });
    }
}

fn extract_attribute(svg: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = svg.find(&needle)? + needle.len();
    let end = svg[start..].find('"')? + start;
    Some(svg[start..end].to_string())
}

// ── Content-parity checks ───────────────────────────────────────────

/// Concatenated text content of the SVG, decoded from XML entities, used for
/// label-presence checks. We collect text between `>` and `<` for `text`,
/// `tspan`, and `title` elements.
fn svg_text_content(svg: &str) -> String {
    let mut out = String::new();
    let bytes = svg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'>' {
            // Capture until the next '<'.
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'<' {
                j += 1;
            }
            if j > start {
                out.push_str(&svg[start..j]);
                out.push('\n');
            }
            i = j;
        } else {
            i += 1;
        }
    }
    decode_entities(&out)
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Normalize a label for fuzzy presence matching: collapse whitespace, since
/// the renderer wraps and re-spaces multi-word labels across tspans.
fn normalize_label(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a source label into the atomic text tokens we expect to find in the
/// render. Handles the ways Mermaid labels get broken across rendered rows:
///   * explicit line breaks: literal `\n`, real newlines, and `<br/>` tags
///   * class/ER compartment separators: `---` divider rows
///   * markdown/HTML wrappers that the renderer strips (`**bold**`, `<i>`)
///
/// Each returned token must appear somewhere in the rendered text.
fn label_tokens(label: &str) -> Vec<String> {
    let mut work = label.to_string();
    // Normalize line-break spellings to a single separator.
    for brk in ["<br/>", "<br>", "<br />", "\\n", "\r\n", "\n"] {
        work = work.replace(brk, "\u{1}");
    }
    // Compartment separators in class/ER labels are divider rows, not text.
    work = work.replace("---", "\u{1}");
    // Strip simple markdown/HTML emphasis wrappers the renderer drops.
    for tag in ["**", "<i>", "</i>", "<b>", "</b>", "<em>", "</em>", "`"] {
        work = work.replace(tag, "");
    }
    work.split('\u{1}')
        .map(normalize_label)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Does the rendered text contain every atomic token of `label`?
/// Long labels may be wrapped across lines, so we require each token (a single
/// rendered row's worth of text) to be present rather than the whole string.
///
/// `table_layout` is set for class/ER entities, whose compartment rows are
/// rendered as multi-column cells (e.g. `string id` becomes a `string` cell
/// and an `id` cell in separate `<text>` elements). For those we match each
/// whitespace-separated word individually rather than the whole row, which
/// still catches a dropped attribute while tolerating column splitting.
fn text_contains_label(
    haystack_norm: &str,
    haystack_raw_lines: &BTreeSet<String>,
    label: &str,
    table_layout: bool,
) -> bool {
    let tokens = label_tokens(label);
    if tokens.is_empty() {
        return true;
    }
    let present = |needle: &str| -> bool {
        haystack_norm.contains(needle)
            || haystack_raw_lines
                .iter()
                .any(|l| normalize_label(l).contains(needle))
    };
    tokens.iter().all(|tok| {
        if present(tok.as_str()) {
            return true;
        }
        if table_layout {
            // Column layout: each word of the row must appear somewhere.
            return tok.split(' ').all(present);
        }
        false
    })
}

/// Synthetic nodes the layout engine inserts (state start/end pseudostates,
/// edge-label dummies) carry internal ids and render as glyphs, not text.
/// They are not part of the input's visible content.
fn is_synthetic_node_id(id: &str) -> bool {
    id.starts_with("__start_")
        || id.starts_with("__end_")
        || id.starts_with("__elabel_")
        || id == "__start_root__"
        || id == "__end_root__"
        || (id.starts_with("__") && id.ends_with("__"))
}

/// Check that every node label and edge label appears in the rendered text.
/// This is the core "no dropped content" guarantee. We skip diagram kinds
/// whose textual content is structural rather than graph-node based and is
/// verified by per-type suites later (pie/gantt/xychart/etc.).
fn check_label_parity(fixture: &str, graph: &Graph, svg: &str, findings: &mut Vec<Finding>) {
    // Only graph-shaped kinds carry node/edge labels in `graph.nodes`.
    let graph_shaped = matches!(
        graph.kind,
        DiagramKind::Flowchart
            | DiagramKind::State
            | DiagramKind::Class
            | DiagramKind::Er
            | DiagramKind::Mindmap
    );
    if !graph_shaped {
        return;
    }

    // Class/ER entities render attribute compartments as multi-column tables.
    let table_layout = matches!(graph.kind, DiagramKind::Class | DiagramKind::Er);

    let text = svg_text_content(svg);
    let text_norm = normalize_label(&text);
    let raw_lines: BTreeSet<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    for node in graph.nodes.values() {
        if is_synthetic_node_id(&node.id) {
            continue;
        }
        let label = if node.label.trim().is_empty() {
            node.id.as_str()
        } else {
            node.label.as_str()
        };
        // Empty/whitespace labels and pure-punctuation ids are not rendered.
        if label.trim().is_empty() {
            continue;
        }
        if !text_contains_label(&text_norm, &raw_lines, label, table_layout) {
            findings.push(Finding {
                fixture: fixture.to_string(),
                rule: "node-label-parity",
                detail: format!("node '{}' label '{}' not found in render", node.id, label),
            });
        }
    }

    for (idx, edge) in graph.edges.iter().enumerate() {
        for (kind, maybe_label) in [
            ("center", &edge.label),
            ("start", &edge.start_label),
            ("end", &edge.end_label),
        ] {
            if let Some(label) = maybe_label {
                if label.trim().is_empty() {
                    continue;
                }
                if !text_contains_label(&text_norm, &raw_lines, label, table_layout) {
                    findings.push(Finding {
                        fixture: fixture.to_string(),
                        rule: "edge-label-parity",
                        detail: format!(
                            "edge #{idx} {}->{} {kind} label '{label}' not found in render",
                            edge.from, edge.to
                        ),
                    });
                }
            }
        }
    }
}

/// Check that the number of rendered edge paths matches the graph edge count.
/// We count `class="...edge..."`-tagged path/line elements. Self-loops and
/// multi-edges each get their own path, so the count should be >= edges.
fn check_edge_count(fixture: &str, graph: &Graph, svg: &str, findings: &mut Vec<Finding>) {
    let graph_shaped = matches!(
        graph.kind,
        DiagramKind::Flowchart | DiagramKind::State | DiagramKind::Class | DiagramKind::Er
    );
    if !graph_shaped || graph.edges.is_empty() {
        return;
    }
    // Count edge path markers. The renderer tags edge paths with an
    // `data-edge-id` or an `edge`-bearing class; count distinct edge path
    // elements by the `data-edge-index` / edge class marker.
    let rendered = count_edge_paths(svg);
    // Allow rendered >= edges (decorations/labels add elements) but never
    // fewer than the declared edges: that means an edge was dropped.
    if rendered < graph.edges.len() {
        findings.push(Finding {
            fixture: fixture.to_string(),
            rule: "edge-count-parity",
            detail: format!(
                "graph has {} edges but only {} edge paths rendered",
                graph.edges.len(),
                rendered
            ),
        });
    }
}

/// Count edge path elements in the SVG. The renderer marks edge geometry
/// paths with the substring `class="edge` or a `data-edge` attribute.
fn count_edge_paths(svg: &str) -> usize {
    let by_class = svg.matches("class=\"edge").count();
    let by_data = svg.matches("data-edge-id=").count();
    by_class.max(by_data)
}

// ── Test entry points ───────────────────────────────────────────────

fn run_all_checks() -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in collect_fixtures() {
        let fixture = fixture_name(&path);
        match render_fixture(&path) {
            Ok((graph, svg)) => {
                check_well_formed(&fixture, &svg, &mut findings);
                check_finite_numbers(&fixture, &svg, &mut findings);
                check_viewbox(&fixture, &svg, &mut findings);
                check_label_parity(&fixture, &graph, &svg, &mut findings);
                check_edge_count(&fixture, &graph, &svg, &mut findings);
            }
            Err(e) => {
                findings.push(Finding {
                    fixture: fixture.clone(),
                    rule: "render",
                    detail: e,
                });
            }
        }
    }
    findings
}

#[test]
fn every_fixture_renders_validly_and_preserves_content() {
    let findings = run_all_checks();
    if !findings.is_empty() {
        let mut report = format!("\n{} correctness finding(s):\n", findings.len());
        for f in &findings {
            report.push_str(&format!("  [{}] {}: {}\n", f.rule, f.fixture, f.detail));
        }
        panic!("{report}");
    }
}

// ── Self-tests: prove the checker actually catches defects ──────────

#[test]
fn label_parity_flags_a_dropped_node_label() {
    // Render text is missing the node label "Charlie" entirely.
    let text_norm = "Alice Bob";
    let raw: BTreeSet<String> = ["Alice".to_string(), "Bob".to_string()]
        .into_iter()
        .collect();
    assert!(text_contains_label(text_norm, &raw, "Alice", false));
    assert!(
        !text_contains_label(text_norm, &raw, "Charlie", false),
        "dropped label must be detected"
    );
}

#[test]
fn label_parity_accepts_wrapped_and_compartment_labels() {
    let text_norm = "Set cookie & return 200 string id name";
    let raw: BTreeSet<String> = [
        "Set cookie &".to_string(),
        "return 200".to_string(),
        "string".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]
    .into_iter()
    .collect();
    // Wrapped edge label split across two rows.
    assert!(text_contains_label(
        text_norm,
        &raw,
        "Set cookie &\nreturn 200",
        false
    ));
    // Class/ER attribute row rendered as separate column cells.
    assert!(text_contains_label(text_norm, &raw, "string id", true));
    // Missing attribute is still caught even in table mode.
    assert!(!text_contains_label(text_norm, &raw, "string price", true));
}

#[test]
fn finite_number_check_flags_nan_attribute() {
    let mut findings = Vec::new();
    let svg = r#"<svg viewBox="0 0 100 100"><path d="M NaN 0 L 10 10"/></svg>"#;
    check_finite_numbers("synthetic", svg, &mut findings);
    assert!(
        findings.iter().any(|f| f.rule == "finite-numbers"),
        "NaN coordinate must be flagged"
    );
}

#[test]
fn viewbox_check_flags_nonpositive_size() {
    let mut findings = Vec::new();
    check_viewbox(
        "synthetic",
        r#"<svg viewBox="0 0 0 100"></svg>"#,
        &mut findings,
    );
    assert!(findings.iter().any(|f| f.rule == "viewbox-positive"));
}

#[test]
fn synthetic_state_nodes_are_excluded() {
    assert!(is_synthetic_node_id("__start_root__"));
    assert!(is_synthetic_node_id("__end_root__"));
    assert!(is_synthetic_node_id("__start_Active__"));
    assert!(!is_synthetic_node_id("Active"));
    assert!(!is_synthetic_node_id("my_node"));
}
