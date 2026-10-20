#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft diagrams (layer L1): Mermaid source found in a page's text, and drawn as a picture.
//!
//! A diagram lives in a page as text, in either format: a ```` ```mermaid ```` fence in a
//! Markdown page, `<pre class="diagram">` or `<pre class="mermaid">` (or a `<div>` with that
//! class) in an HTML page. [`find`] reads them out of a page's text. [`draw`] turns one into an
//! SVG picture with `mermaid-rs-renderer`, a pure Rust renderer, in the colours of the page.
//!
//! Nothing here is trusted. A source is cut by size before it reaches the renderer, a renderer
//! that panics is caught, and a failure is an error message, never a crash: the window then shows
//! the source, which is never lost. This crate has no UI dependency.

use mermaid_rs_renderer::{RenderOptions, Theme, render_with_options};

/// The longest diagram, in characters, that is drawn. The renderer's layout grows quickly with
/// the size of a graph: a 300-node flowchart took over two seconds in the survey.
pub const MAX_SOURCE_CHARS: usize = 20_000;
/// The most lines a diagram may have and still be drawn.
pub const MAX_LINES: usize = 150;
/// The largest a drawing may be on either side, in pixels at its own size. A picture bigger than
/// the graphics card's textures cannot be shown at all, and a window that asks for one panics.
pub const MAX_SIDE: f32 = 16_000.0;

/// The colours a diagram is drawn in, as CSS colours, so it belongs to the page's theme.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    /// Behind the whole drawing.
    pub background: String,
    /// Inside a box.
    pub fill: String,
    /// Round a box.
    pub border: String,
    /// Writing.
    pub text: String,
    /// Arrows and lines.
    pub line: String,
    /// Behind the words on an arrow.
    pub label_background: String,
    /// Inside a group of boxes, and a note.
    pub cluster_fill: String,
    /// Round a group of boxes, and a note.
    pub cluster_border: String,
}

/// The colours of the series in a chart, in order.
const SERIES: [&str; 10] = ["#4C78A8", "#F58518", "#E45756", "#72B7B2", "#54A24B", "#EECA3B", "#B279A2", "#FF9DA6", "#9D755D", "#BAB0AC"];

impl Default for Palette {
    fn default() -> Self {
        Palette {
            background: "#FFFFFF".into(),
            fill: "#EEEAF7".into(),
            border: "#7A52B3".into(),
            text: "#1E1A2B".into(),
            line: "#3A3550".into(),
            label_background: "#FFFFFF".into(),
            cluster_fill: "#F6F3FB".into(),
            cluster_border: "#B7A6D8".into(),
        }
    }
}

impl Palette {
    fn theme(&self) -> Theme {
        let mut t = Theme::modern();
        t.background = self.background.clone();
        t.primary_color = self.fill.clone();
        t.primary_text_color = self.text.clone();
        t.primary_border_color = self.border.clone();
        t.line_color = self.line.clone();
        t.text_color = self.text.clone();
        t.edge_label_background = self.label_background.clone();
        t.cluster_background = self.cluster_fill.clone();
        t.cluster_border = self.cluster_border.clone();
        t.sequence_actor_fill = self.fill.clone();
        t.sequence_actor_border = self.border.clone();
        t.sequence_actor_line = self.line.clone();
        t.sequence_note_fill = self.cluster_fill.clone();
        t.sequence_note_border = self.cluster_border.clone();
        t.pie_title_text_color = self.text.clone();
        t.pie_section_text_color = self.text.clone();
        t.pie_legend_text_color = self.text.clone();
        t.pie_stroke_color = self.background.clone();
        // A pie's slices are chart colours, not the page's: ten that tell apart on a light page and on
        // a dark one (the Tableau ten), so a slice never fades into the background.
        t.pie_colors = std::array::from_fn(|i| SERIES[i % SERIES.len()].to_string());
        t
    }
}

/// Draws one diagram as an SVG document, or says why it cannot.
pub fn draw(source: &str, palette: &Palette) -> Result<String, String> {
    if source.trim().is_empty() {
        return Err("The diagram is empty".to_string());
    }
    if source.chars().count() > MAX_SOURCE_CHARS {
        return Err(format!("The diagram is longer than {MAX_SOURCE_CHARS} characters, which is as much as is drawn"));
    }
    if source.lines().count() > MAX_LINES {
        return Err(format!("The diagram has more than {MAX_LINES} lines, which is as much as is drawn"));
    }
    let options = RenderOptions { theme: palette.theme(), ..RenderOptions::default() };
    let owned = source.to_owned();
    // The renderer is young. Whatever it does with a diagram it was not made for, the window goes on.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || render_with_options(&owned, options))) {
        Ok(Ok(svg)) => match size(&svg) {
            Some((w, h)) if w > 0.0 && h > 0.0 && w <= MAX_SIDE && h <= MAX_SIDE => Ok(svg),
            Some(_) => Err("The drawing would be too large to show".to_string()),
            None => Err("The drawing has no size".to_string()),
        },
        Ok(Err(e)) => Err(first_line(&e.to_string())),
        Err(_) => Err("The renderer could not draw this diagram".to_string()),
    }
}

/// The width and height of an SVG document, from its `viewBox` or its `width` and `height`.
pub fn size(svg: &str) -> Option<(f32, f32)> {
    let open = &svg[svg.find("<svg")?..];
    let tag = &open[..open.find('>')?];
    let attr = |name: &str| -> Option<&str> {
        let mut from = 0;
        while let Some(i) = tag[from..].find(name) {
            let at = from + i;
            let before = tag[..at].chars().last();
            let rest = tag[at + name.len()..].trim_start();
            if before.is_some_and(char::is_whitespace) && rest.starts_with('=') {
                let value = rest[1..].trim_start();
                let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
                return value[1..].split(quote).next();
            }
            from = at + name.len();
        }
        None
    };
    let number = |s: &str| s.trim().trim_end_matches("px").parse::<f32>().ok().filter(|v| v.is_finite());
    if let Some(vb) = attr("viewBox") {
        let n: Vec<f32> = vb.split([' ', ',']).filter(|p| !p.is_empty()).filter_map(number).collect();
        if let [_, _, w, h] = n[..] {
            return Some((w, h));
        }
    }
    Some((number(attr("width")?)?, number(attr("height")?)?))
}

/// By how much a drawing of `w` by `h` is to be scaled to be shown in `room` points of width, never
/// larger than its own size and never so large that its picture is past `max_side` pixels on a
/// screen with `pixels_per_point` pixels to the point.
pub fn fit(w: f32, h: f32, room: f32, max_side: f32, pixels_per_point: f32) -> f32 {
    if !(w > 0.0 && h > 0.0) {
        return 1.0;
    }
    let limit = (max_side / pixels_per_point.max(0.25)).max(64.0);
    let mut scale = 1.0_f32;
    if room.is_finite() && room > 1.0 {
        scale = scale.min(room / w);
    }
    scale.min(limit / w).min(limit / h)
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("The diagram cannot be read").trim().to_string()
}

/// Every Mermaid diagram in a page's text, in the order they come, as their source.
pub fn find(text: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    fences(text, &mut found);
    tags(text, &mut found);
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, source)| source).collect()
}

/// A Markdown fence: ```` ```mermaid ```` or `~~~mermaid`. A fence of any other language is
/// skipped whole, so a Mermaid example shown inside it is not taken for a diagram.
fn fences(text: &str, out: &mut Vec<(usize, String)>) {
    let mut offset = 0;
    let mut open: Option<(char, usize, bool, usize, String)> = None; // marker, length, mermaid, start, body
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        let trimmed = content.trim_start();
        let indent = content.len() - trimmed.len();
        match open.take() {
            None => {
                let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
                if let Some(m) = marker.filter(|_| indent <= 3) {
                    let n = trimmed.chars().take_while(|c| *c == m).count();
                    if n >= 3 {
                        let lang = trimmed[n..].split_whitespace().next().unwrap_or("");
                        open = Some((m, n, lang.eq_ignore_ascii_case("mermaid"), start, String::new()));
                    }
                }
            }
            Some((m, n, mermaid, at, mut body)) => {
                let closes = indent <= 3 && trimmed.chars().take_while(|c| *c == m).count() >= n && trimmed.trim_start_matches(m).trim().is_empty();
                if closes {
                    if mermaid && !body.trim().is_empty() {
                        out.push((at, body.trim_end().to_string()));
                    }
                } else {
                    body.push_str(content);
                    body.push('\n');
                    open = Some((m, n, mermaid, at, body));
                }
            }
        }
    }
}

/// An HTML element that holds a diagram: a `pre` with class `diagram` or `mermaid`, or a `div`
/// with class `mermaid`. The text inside is the source, with its entities read back.
fn tags(text: &str, out: &mut Vec<(usize, String)>) {
    let lower = text.to_ascii_lowercase();
    for (name, classes) in [("pre", &["diagram", "mermaid"][..]), ("div", &["mermaid"][..])] {
        let open_tag = format!("<{name}");
        let close_tag = format!("</{name}>");
        let mut from = 0;
        while let Some(rel) = lower[from..].find(&open_tag) {
            let at = from + rel;
            let after = at + open_tag.len();
            from = after;
            // `<pre` and not `<prefix`: the next character ends the tag's name.
            if !lower[after..].starts_with([' ', '\t', '\n', '\r', '>']) {
                continue;
            }
            let Some(gt) = lower[after..].find('>') else { break };
            let attrs = &lower[after..after + gt];
            if !class_has(attrs, classes) {
                continue;
            }
            let body_start = after + gt + 1;
            let Some(end) = lower[body_start..].find(&close_tag) else { continue };
            let source = decode(&text[body_start..body_start + end]);
            if !source.trim().is_empty() {
                out.push((at, source.trim().to_string()));
            }
            from = body_start + end + close_tag.len();
        }
    }
}

/// Whether the `class="..."` in a tag's attributes holds one of `wanted` as a whole word.
fn class_has(attrs: &str, wanted: &[&str]) -> bool {
    let Some(i) = attrs.find("class") else { return false };
    let rest = attrs[i + 5..].trim_start();
    let Some(rest) = rest.strip_prefix('=') else { return false };
    let rest = rest.trim_start();
    let (quote, rest) = match rest.chars().next() {
        Some(q @ ('"' | '\'')) => (Some(q), &rest[1..]),
        _ => (None, rest),
    };
    let value = match quote {
        Some(q) => rest.split(q).next().unwrap_or(""),
        None => rest.split_whitespace().next().unwrap_or(""),
    };
    value.split_whitespace().any(|word| wanted.contains(&word))
}

/// The entities an HTML page writes a diagram with: `-->` is `--&gt;` there.
fn decode(s: &str) -> String {
    s.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_markdown_fence_is_found() {
        let md = "# Title\n\n```mermaid\ngraph TD;\n  A-->B;\n```\n\ntext\n";
        assert_eq!(find(md), vec!["graph TD;\n  A-->B;".to_string()]);
    }

    #[test]
    fn tilde_fences_and_a_capital_name_and_indentation_are_found() {
        assert_eq!(find("~~~Mermaid\nflowchart LR; A-->B\n~~~\n"), vec!["flowchart LR; A-->B".to_string()]);
        assert_eq!(find("   ```mermaid\npie\n   ```\n").len(), 1);
    }

    #[test]
    fn other_fences_are_skipped_whole_and_so_is_a_diagram_shown_inside_one() {
        let md = "```rust\nfn main() {}\n```\n\n````text\n```mermaid\ngraph TD; A-->B\n```\n````\n";
        assert!(find(md).is_empty(), "{:?}", find(md));
    }

    #[test]
    fn an_unclosed_fence_is_not_a_diagram_and_does_not_hang() {
        assert!(find("```mermaid\ngraph TD; A-->B\n").is_empty());
    }

    #[test]
    fn html_pre_diagram_and_pre_mermaid_and_div_mermaid_are_found_with_their_entities_read() {
        let html = "<p>x</p><pre class=\"diagram\">graph TD; A--&gt;B;</pre>\n\
                    <pre class=\"mermaid\">graph LR\n  X--&gt;Y</pre>\n\
                    <div class=\"mermaid\">pie\n  \"a\" : 1</div>";
        assert_eq!(find(html), vec!["graph TD; A-->B;".to_string(), "graph LR\n  X-->Y".to_string(), "pie\n  \"a\" : 1".to_string()]);
    }

    #[test]
    fn other_html_is_not_a_diagram() {
        assert!(
            find("<pre class=\"code\">graph TD</pre><pre>graph TD</pre><div class=\"mermaidx\">a</div><prefix class=\"mermaid\">a</prefix>")
                .is_empty()
        );
        assert!(find("<pre class=\"mermaid\">unclosed").is_empty());
    }

    #[test]
    fn a_page_with_several_gives_them_in_order() {
        let md = "```mermaid\nA\n```\ntext\n```mermaid\nB\n```\n";
        assert_eq!(find(md), vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn the_same_diagram_in_both_formats_gives_the_same_source() {
        let md = "```mermaid\nflowchart TD\n  A[Start] --> B{Plain?}\n```\n";
        let html = "<pre class=\"diagram\">flowchart TD\n  A[Start] --&gt; B{Plain?}</pre>";
        assert_eq!(find(md), find(html));
    }

    #[test]
    fn a_flowchart_is_drawn_as_svg_with_its_words() {
        let svg = draw("flowchart TD\n  A[Start] --> B{Is it plain?}\n  B -->|yes| C[Keep it]", &Palette::default());
        let svg = svg.unwrap_or_default();
        assert!(svg.contains("<svg"), "{svg}");
        for word in ["Start", "Is it plain?", "Keep it", "yes"] {
            assert!(svg.contains(word), "{word} is in the drawing");
        }
    }

    #[test]
    fn the_palette_is_the_pages_colours() {
        let palette = Palette { fill: "#123456".into(), border: "#ABCDEF".into(), ..Palette::default() };
        let svg = draw("flowchart LR; A-->B", &palette).unwrap_or_default();
        assert!(svg.to_ascii_lowercase().contains("#123456"), "the box fill is ours");
        assert!(svg.to_ascii_lowercase().contains("#abcdef"), "the box border is ours");
    }

    #[test]
    fn the_kinds_of_diagram_the_survey_drew_are_drawn() {
        for source in [
            "sequenceDiagram\n  U->>N: Type\n  N-->>U: Saved",
            "pie title Pages\n  \"HTML\" : 62\n  \"Markdown\" : 31",
            "stateDiagram-v2\n  [*] --> Clean\n  Clean --> Dirty: type",
            "classDiagram\n  class Page\n  Page <|-- HtmlPage",
            "erDiagram\n  NOTEBOOK ||--o{ SECTION : holds",
            "gantt\n  title Release\n  dateFormat YYYY-MM-DD\n  section Build\n  Band :a1, 2026-10-12, 5d",
            "mindmap\n  root((NoteCraft))\n    Pages\n    Ink",
        ] {
            let svg = draw(source, &Palette::default());
            assert!(svg.as_ref().is_ok_and(|s| s.contains("<svg")), "{source}: {svg:?}");
        }
    }

    #[test]
    fn arabic_labels_are_in_the_drawing() {
        let svg = draw("flowchart RL\n  A[البداية] --> B{هل النص عادي؟}", &Palette::default()).unwrap_or_default();
        assert!(svg.contains("البداية"), "{svg}");
    }

    #[test]
    fn a_drawings_size_is_read_from_its_view_box_or_its_width_and_height() {
        assert_eq!(
            size(r#"<svg xmlns="http://www.w3.org/2000/svg" width="315.5" height="521.25" viewBox="0 0 315.5 521.25">"#),
            Some((315.5, 521.25))
        );
        assert_eq!(size(r#"<svg width="100px" height="50px">"#), Some((100.0, 50.0)));
        assert_eq!(size(r#"<svg viewBox='0,0,40,30'>"#), Some((40.0, 30.0)));
        assert_eq!(size("<svg>"), None);
        assert_eq!(size("no svg here"), None);
        assert_eq!(size(r#"<svg data-width="5" width="10" height="20">"#), Some((10.0, 20.0)), "a name that merely ends in width is not width");
    }

    #[test]
    fn a_drawing_is_scaled_to_the_room_and_never_past_what_a_texture_can_hold() {
        assert_eq!(fit(300.0, 200.0, 1000.0, 2048.0, 1.0), 1.0, "never larger than itself");
        assert!((fit(1000.0, 500.0, 500.0, 2048.0, 1.0) - 0.5).abs() < 1e-6, "narrowed to the room");
        // 151 wide and 22500 tall, which a window once asked the graphics card to hold whole.
        let s = fit(151.0, 22500.0, 400.0, 2048.0, 1.0);
        assert!(22500.0 * s <= 2048.0 + 0.5, "{s}");
        // A screen that draws two pixels to the point halves what fits.
        let s = fit(3000.0, 3000.0, f32::INFINITY, 2048.0, 2.0);
        assert!(3000.0 * s * 2.0 <= 2048.0 + 0.5, "{s}");
        assert_eq!(fit(0.0, 10.0, 100.0, 2048.0, 1.0), 1.0, "nonsense in, a plain scale out");
    }

    #[test]
    fn what_cannot_be_drawn_says_why_and_never_panics() {
        assert!(draw("", &Palette::default()).is_err());
        assert!(draw("   \n", &Palette::default()).is_err());
        let broken = draw("flowchart TD\n  A[unclosed --> B(\n  C -->\n  graph ??? \n", &Palette::default());
        assert!(broken.is_err(), "{broken:?}");
        let long = "flowchart TD\n".to_string() + &"  A --> B\n".repeat(MAX_LINES);
        assert!(draw(&long, &Palette::default()).is_err_and(|e| e.contains("lines")));
        let huge = "a".repeat(MAX_SOURCE_CHARS + 1);
        assert!(draw(&huge, &Palette::default()).is_err_and(|e| e.contains("characters")));
        for odd in ["\u{0}", "\u{202e}x", "graph", "pie\n\"a\" : -1", "sequenceDiagram\n->>", "gantt\ndateFormat nonsense"] {
            let _ = draw(odd, &Palette::default());
        }
    }
}
