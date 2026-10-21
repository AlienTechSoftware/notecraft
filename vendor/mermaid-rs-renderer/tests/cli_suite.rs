use std::io::Write;
use std::process::{Command, Stdio};

fn mmdr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mmdr"))
}

fn assert_svg_size(svg: &str, width: &str, height: &str) {
    let root = svg
        .split('>')
        .next()
        .expect("SVG output should include a root element");
    assert!(
        root.contains(&format!("width=\"{width}\"")),
        "expected width={width:?} in root element: {root}"
    );
    assert!(
        root.contains(&format!("height=\"{height}\"")),
        "expected height={height:?} in root element: {root}"
    );
}

#[test]
fn cli_width_height_affect_stdout_svg() {
    let mut child = mmdr()
        .args(["--width", "321", "--height", "123", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart TD\n  A-->B\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");

    assert!(
        output.status.success(),
        "mmdr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = String::from_utf8(output.stdout).expect("SVG stdout should be UTF-8");
    assert_svg_size(&svg, "321", "123");
}

/// Issue #83: without explicit --width/--height the root svg width/height must
/// equal the natural viewBox dimensions (no letterboxing).
#[test]
fn cli_default_svg_uses_natural_dimensions() {
    let mut child = mmdr()
        .args(["--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart TD\n  A-->B\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");

    assert!(
        output.status.success(),
        "mmdr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = String::from_utf8(output.stdout).expect("SVG stdout should be UTF-8");
    let root = svg
        .split('>')
        .next()
        .expect("SVG output should include a root element");
    let attr = |name: &str| -> String {
        let start = root
            .find(&format!("{name}=\""))
            .unwrap_or_else(|| panic!("missing {name} in root element: {root}"))
            + name.len()
            + 2;
        root[start..]
            .split('"')
            .next()
            .expect("attribute should be terminated")
            .to_string()
    };
    let width: f32 = attr("width").parse().expect("width should be numeric");
    let height: f32 = attr("height").parse().expect("height should be numeric");
    let viewbox = attr("viewBox");
    let parts: Vec<f32> = viewbox
        .split_whitespace()
        .map(|v| v.parse().expect("viewBox values should be numeric"))
        .collect();
    assert_eq!(parts.len(), 4, "viewBox should have 4 values: {viewbox}");
    assert!(
        (width - parts[2]).abs() < 0.01,
        "default width {width} should match viewBox width {}",
        parts[2]
    );
    assert!(
        (height - parts[3]).abs() < 0.01,
        "default height {height} should match viewBox height {}",
        parts[3]
    );
    // Guard against reverting to the fixed 1200x800 letterbox default.
    assert!(
        width != 1200.0 || height != 800.0,
        "default dims should be natural, not the 1200x800 fallback"
    );
}

/// Issue #83: default PNG output should rasterize at the natural aspect ratio,
/// not letterboxed into 1200x800.
#[cfg(feature = "png")]
#[test]
fn cli_default_png_has_no_letterbox() {
    let dir = std::env::temp_dir().join(format!(
        "mermaid-rs-renderer-cli-png-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let input = dir.join("input.mmd");
    let svg_path = dir.join("output.svg");
    let png_path = dir.join("output.png");
    std::fs::write(&input, "flowchart TD\n  A-->B\n").expect("failed to write input");

    let svg_out = mmdr()
        .args([
            "--input",
            input.to_str().unwrap(),
            "--output",
            svg_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run mmdr (svg)");
    assert!(
        svg_out.status.success(),
        "mmdr svg failed: {}",
        String::from_utf8_lossy(&svg_out.stderr)
    );
    let png_out = mmdr()
        .args([
            "--input",
            input.to_str().unwrap(),
            "--output",
            png_path.to_str().unwrap(),
            "-e",
            "png",
        ])
        .output()
        .expect("failed to run mmdr (png)");
    assert!(
        png_out.status.success(),
        "mmdr png failed: {}",
        String::from_utf8_lossy(&png_out.stderr)
    );

    // Natural SVG dimensions.
    let svg = std::fs::read_to_string(&svg_path).expect("failed to read SVG output");
    let root = svg.split('>').next().expect("missing root element");
    let attr = |name: &str| -> f32 {
        let start = root
            .find(&format!("{name}=\""))
            .unwrap_or_else(|| panic!("missing {name} in root element: {root}"))
            + name.len()
            + 2;
        root[start..]
            .split('"')
            .next()
            .expect("attribute should be terminated")
            .parse()
            .expect("attribute should be numeric")
    };
    let svg_aspect = attr("width") / attr("height");

    // PNG dimensions from the IHDR chunk.
    let png = std::fs::read(&png_path).expect("failed to read PNG output");
    let png_w = u32::from_be_bytes(png[16..20].try_into().unwrap()) as f32;
    let png_h = u32::from_be_bytes(png[20..24].try_into().unwrap()) as f32;
    let png_aspect = png_w / png_h;

    assert!(
        (svg_aspect - png_aspect).abs() < 0.05,
        "PNG aspect {png_aspect} should match natural SVG aspect {svg_aspect} (no letterbox), png {png_w}x{png_h}"
    );
    let letterbox_aspect = 1200.0 / 800.0;
    assert!(
        (png_aspect - letterbox_aspect).abs() > 0.05,
        "PNG should not be letterboxed to 1200x800 ({png_w}x{png_h})"
    );

    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(svg_path);
    let _ = std::fs::remove_file(png_path);
    let _ = std::fs::remove_dir(dir);
}

/// Issue #101: `-i -` reads a diagram from stdin and writes to a file.
#[test]
fn cli_stdin_input_renders_to_file() {
    let dir = std::env::temp_dir().join(format!(
        "mermaid-rs-renderer-cli-stdin-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let output_path = dir.join("stdin-output.svg");

    let mut child = mmdr()
        .args(["-i", "-", "-o", output_path.to_str().unwrap(), "-e", "svg"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart LR\n  Start --> End\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");

    assert!(
        output.status.success(),
        "mmdr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = std::fs::read_to_string(&output_path).expect("failed to read SVG output");
    assert!(svg.starts_with("<svg"), "output should be an SVG document");
    assert!(svg.contains("Start"), "SVG should contain node label");
    assert!(svg.contains("End"), "SVG should contain node label");

    let _ = std::fs::remove_file(output_path);
    let _ = std::fs::remove_dir(dir);
}

#[test]
fn cli_width_height_affect_file_svg() {
    let dir = std::env::temp_dir().join(format!(
        "mermaid-rs-renderer-cli-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let input = dir.join("input.mmd");
    let output_path = dir.join("output.svg");
    std::fs::write(&input, "flowchart TD\n  A-->B\n").expect("failed to write input");

    let output = mmdr()
        .args([
            "--width",
            "654",
            "--height",
            "456",
            "--input",
            input.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run mmdr");

    assert!(
        output.status.success(),
        "mmdr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = std::fs::read_to_string(&output_path).expect("failed to read SVG output");
    assert_svg_size(&svg, "654", "456");

    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(output_path);
    let _ = std::fs::remove_dir(dir);
}

/// Issue #73: named theme presets selectable via --theme.
#[test]
fn cli_theme_flag_selects_builtin_presets() {
    for (name, expected_bg) in [
        ("dark", "#333333"),
        ("forest", "#ffffff"),
        ("neutral", "#ffffff"),
        ("default", "#FFFFFF"),
    ] {
        let mut child = mmdr()
            .args(["--theme", name, "--input", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to run mmdr");
        child
            .stdin
            .as_mut()
            .expect("failed to open stdin")
            .write_all(b"flowchart TD\n  A-->B\n")
            .expect("failed to write stdin");
        let output = child.wait_with_output().expect("failed to wait for mmdr");
        assert!(
            output.status.success(),
            "mmdr --theme {name} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let svg = String::from_utf8(output.stdout).expect("SVG stdout should be UTF-8");
        assert!(
            svg.contains(&format!("fill=\"{expected_bg}\"")),
            "--theme {name} should set background {expected_bg}"
        );
    }

    // Dark preset should color nodes with its primary color.
    let mut child = mmdr()
        .args(["--theme", "dark", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart TD\n  A-->B\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");
    let svg = String::from_utf8(output.stdout).expect("SVG stdout should be UTF-8");
    assert!(
        svg.contains("#1f2020"),
        "dark theme should use #1f2020 node fills"
    );
}

/// Issue #73: unknown preset names should fail with a helpful error.
#[test]
fn cli_theme_flag_rejects_unknown_names() {
    let mut child = mmdr()
        .args(["--theme", "does-not-exist", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart TD\n  A-->B\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");
    assert!(
        !output.status.success(),
        "unknown theme names should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown theme preset"),
        "error should mention the unknown preset: {stderr}"
    );
}

/// Issue #73: themeVariables from the config file must still override the
/// preset selected via --theme.
#[test]
fn cli_theme_flag_composes_with_theme_variables() {
    let dir = std::env::temp_dir().join("mmdr-theme-flag-test");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let config_path = dir.join("config.json");
    std::fs::write(
        &config_path,
        r##"{"themeVariables": {"primaryColor": "#123456"}}"##,
    )
    .expect("write config");

    let mut child = mmdr()
        .args([
            "--theme",
            "dark",
            "--configFile",
            config_path.to_str().expect("utf-8 path"),
            "--input",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run mmdr");
    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(b"flowchart TD\n  A-->B\n")
        .expect("failed to write stdin");
    let output = child.wait_with_output().expect("failed to wait for mmdr");
    assert!(
        output.status.success(),
        "mmdr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = String::from_utf8(output.stdout).expect("SVG stdout should be UTF-8");
    assert!(
        svg.contains("#123456"),
        "themeVariables.primaryColor should override the preset"
    );
    assert!(
        svg.contains("fill=\"#333333\""),
        "non-overridden dark preset values (background) should remain"
    );
}
