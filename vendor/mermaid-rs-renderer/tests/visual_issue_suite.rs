use mermaid_rs_renderer::{
    DiagramKind, LayoutConfig, NodeShape, Theme, compute_layout, parse_mermaid, render_svg,
};

fn render(
    input: &str,
) -> (
    mermaid_rs_renderer::Graph,
    mermaid_rs_renderer::Layout,
    String,
) {
    let parsed = parse_mermaid(input).expect("diagram should parse");
    let theme = Theme::modern();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);
    (parsed.graph, layout, svg)
}

#[test]
fn architecture_iconify_icons_render_as_symbols_not_broken_question_marks() {
    let input = r#"architecture-beta
    group api(logos:aws-lambda)[API]

    service db(logos:aws-aurora)[Database] in api
    service disk1(logos:aws-glacier)[Storage] in api
    service disk2(logos:aws-s3)[Storage] in api
    service server(logos:aws-ec2)[Server] in api

    db:L -- R:server
    disk1:T -- B:server
    disk2:T -- B:db
"#;

    let (graph, layout, svg) = render(input);
    assert_eq!(graph.kind, DiagramKind::Architecture);
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(layout.edges.len(), 3);
    assert!(
        !svg.contains(">?</text>") && !svg.contains(">?</tspan>"),
        "registered/Iconify icons should not render as broken question marks"
    );
    assert!(
        svg.contains('λ'),
        "lambda icon should get a symbolic fallback"
    );
    // Port directions must drive grid placement (issue #112): server sits to
    // the left of db (db:L -- R:server), disk1 below server, disk2 below db.
    let node = |id: &str| layout.nodes.get(id).expect(id);
    let (db, server, disk1, disk2) = (node("db"), node("server"), node("disk1"), node("disk2"));
    assert!(
        server.x + server.width <= db.x + 1.0,
        "server should be left of db"
    );
    assert!(
        disk1.y >= server.y + server.height - 1.0,
        "disk1 should be below server"
    );
    assert!(
        disk2.y >= db.y + db.height - 1.0,
        "disk2 should be below db"
    );
}

#[test]
fn architecture_group_edge_modifiers_do_not_create_phantom_nodes() {
    let input = r#"architecture-beta
    group groupOne(cloud)[One]
    group groupTwo(cloud)[Two]
    service server(server)[Server] in groupOne
    service subnet(database)[Subnet] in groupTwo
    server{group}:B --> T:subnet{group}
"#;

    let (graph, layout, svg) = render(input);
    assert!(graph.nodes.contains_key("server"));
    assert!(graph.nodes.contains_key("subnet"));
    assert!(
        graph.nodes.keys().all(|id| !id.contains("{group}")),
        "{{group}} edge modifiers must not become phantom service ids"
    );
    assert_eq!(graph.edges[0].from, "server");
    assert_eq!(graph.edges[0].to, "subnet");
    assert_eq!(layout.nodes.len(), 2);
    assert!(svg.contains("marker-end"));
}

#[test]
fn architecture_junctions_are_compact_routing_points() {
    let input = r#"architecture-beta
    service left_disk(disk)[Disk]
    service top_gateway(internet)[Gateway]
    junction junctionCenter
    junction junctionRight

    left_disk:R -- L:junctionCenter
    junctionCenter:R -- L:junctionRight
    top_gateway:B -- T:junctionRight
"#;

    let (graph, layout, svg) = render(input);
    let center = graph.nodes.get("junctionCenter").expect("junction parsed");
    assert_eq!(center.shape, NodeShape::Circle);
    assert_eq!(center.icon.as_deref(), Some("junction"));

    let center_layout = layout
        .nodes
        .get("junctionCenter")
        .expect("junction laid out");
    assert!(
        center_layout.width <= 24.0 && center_layout.height <= 24.0,
        "junctions should be compact routing dots, got {}x{}",
        center_layout.width,
        center_layout.height
    );
    assert!(svg.contains("<circle"));
    assert!(
        !svg.contains(">junctionCenter<"),
        "junction ids should not render as service labels"
    );
}

#[test]
fn display_math_labels_are_rendered_readably_in_svg_text() {
    let input = r#"graph LR
      A["$$x^2$$"] -->|"$$\sqrt{x+3}$$"| B("$$\frac{1}{2}$$")
      A -->|"$$\overbrace{a+b+c}^{\text{note}}$$"| C("$$\pi r^2$$")
"#;

    let (_graph, _layout, svg) = render(input);
    assert!(svg.contains("x²"));
    assert!(svg.contains("√"));
    assert!(svg.contains("(1)/(2)"));
    assert!(svg.contains("π r²"));
    assert!(
        !svg.contains("$$") && !svg.contains("\\sqrt") && !svg.contains("\\frac"),
        "raw TeX delimiters/commands should not leak into visible SVG text"
    );
}

/// Issue #69: with the default theme the derived pie palette used
/// tertiary == primary, so "Dogs" and "Rats" got the same fill.
#[test]
fn pie_slices_get_distinct_colors_with_default_theme() {
    let input = r#"pie
"Dogs" : 386
"Cats" : 85.9
"Rats" : 15
"#;
    let parsed = parse_mermaid(input).expect("diagram should parse");
    for theme in [Theme::mermaid_default(), Theme::modern()] {
        let config = LayoutConfig::default();
        let layout = compute_layout(&parsed.graph, &theme, &config);
        let mermaid_rs_renderer::layout::DiagramData::Pie(pie) = &layout.diagram else {
            panic!("expected pie layout");
        };
        let colors: Vec<&str> = pie.slices.iter().map(|s| s.color.as_str()).collect();
        let mut deduped = colors.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            colors.len(),
            "pie slices should get distinct palette colors, got {colors:?}"
        );
    }
}

/// Issue #69: the small-slice outside label ("Rats") overlapped the legend
/// because layout and render used different formulas for the label extent.
#[test]
fn pie_outside_label_background_does_not_overlap_legend() {
    let input = r#"pie
"Dogs" : 386
"Cats" : 85.9
"Rats" : 15
"#;
    let parsed = parse_mermaid(input).expect("diagram should parse");
    let theme = Theme::mermaid_default();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);

    // Legend marker rects are 14x14; find the leftmost legend x.
    let mut legend_left = f32::INFINITY;
    let mut label_rect_right = 0.0f32;
    for rect in svg.split("<rect ").skip(1) {
        let attr = |name: &str| -> Option<f32> {
            let key = format!("{name}=\"");
            let start = rect.find(&key)? + key.len();
            let end = start + rect[start..].find('"')?;
            rect[start..end].parse::<f32>().ok()
        };
        let (Some(x), Some(w)) = (attr("x"), attr("width")) else {
            continue;
        };
        if (w - 14.0).abs() < 0.01 {
            legend_left = legend_left.min(x);
        } else if rect.contains("rx=\"2\"") {
            label_rect_right = label_rect_right.max(x + w);
        }
    }
    assert!(legend_left.is_finite(), "legend rects should render");
    assert!(
        label_rect_right > 0.0,
        "outside label background should render"
    );
    assert!(
        label_rect_right <= legend_left,
        "outside pie label background (right edge {label_rect_right}) must not overlap the legend (left edge {legend_left})"
    );
}

/// Issue #112: wide CJK pie titles clipped at both sides of the viewbox
/// because layout ignored the measured title width.
#[test]
fn pie_cjk_title_fits_inside_viewbox() {
    let input = "pie\n    title 这是一个非常非常非常非常长的标题文字测试标题文字测试\n    \"甲\" : 40\n    \"乙\" : 60\n";
    let parsed = parse_mermaid(input).expect("diagram should parse");
    let theme = Theme::mermaid_default();
    let config = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let mermaid_rs_renderer::layout::DiagramData::Pie(pie) = &layout.diagram else {
        panic!("expected pie layout");
    };
    let title = pie.title.as_ref().expect("title should be laid out");
    let left = title.x - title.text.width / 2.0;
    let right = title.x + title.text.width / 2.0;
    assert!(
        left >= 0.0,
        "title should not clip on the left: left edge {left}"
    );
    assert!(
        right <= layout.width,
        "title should not clip on the right: right edge {right}, layout width {}",
        layout.width
    );
}

/// Issue #49: `mindmap.edgeColor` config should force all mindmap edge
/// strokes to one color, independent of the section palette.
#[test]
fn mindmap_edge_color_config_overrides_section_palette() {
    let input = r#"mindmap
  root((Root))
    A
      A1
    B
      B1
"#;
    let parsed = parse_mermaid(input).expect("diagram should parse");
    let theme = Theme::mermaid_default();

    let default_config = LayoutConfig::default();
    let default_layout = compute_layout(&parsed.graph, &theme, &default_config);
    let default_strokes: Vec<String> = default_layout
        .edges
        .iter()
        .filter_map(|edge| edge.override_style.stroke.clone())
        .collect();
    assert!(
        default_strokes.iter().any(|s| s != "#ff00aa"),
        "default mindmap edges should use palette colors"
    );

    let mut config = LayoutConfig::default();
    config.mindmap.edge_color = Some("#ff00aa".to_string());
    let layout = compute_layout(&parsed.graph, &theme, &config);
    assert!(!layout.edges.is_empty(), "mindmap should produce edges");
    for edge in &layout.edges {
        assert_eq!(
            edge.override_style.stroke.as_deref(),
            Some("#ff00aa"),
            "every mindmap edge stroke should use the configured edgeColor"
        );
    }
    let svg = render_svg(&layout, &theme, &config);
    assert!(
        svg.contains("stroke=\"#ff00aa\""),
        "configured mindmap edgeColor should appear in the SVG output"
    );
}
