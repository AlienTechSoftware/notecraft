//! Tree to drawing items. Coordinates are in the same unit as the font size, with the
//! origin at the top left of the formula and y growing downwards.

use crate::parse::{BigKind, Node};

/// How wide a piece of text is. The front end supplies this from its fonts.
pub trait Metrics {
    fn width(&self, text: &str, size: f32, italic: bool) -> f32;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Text with its left edge at `x` and its baseline at `y`.
    Text { x: f32, y: f32, text: String, size: f32, italic: bool },
    /// A straight line.
    Line { from: (f32, f32), to: (f32, f32), width: f32 },
    /// A polyline (the radical sign, the sum and integral signs).
    Path { points: Vec<(f32, f32)>, width: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub width: f32,
    /// Distance from the top to the baseline.
    pub ascent: f32,
    pub descent: f32,
    pub items: Vec<Item>,
}

impl Layout {
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// A box under construction: the origin is its left edge on the baseline.
struct B {
    w: f32,
    asc: f32,
    desc: f32,
    items: Vec<Item>,
}

const ASC: f32 = 0.78;
const DESC: f32 = 0.22;
const AXIS: f32 = 0.30;
const SCRIPT: f32 = 0.70;
const MAX_DEPTH: usize = 64;

fn shift(items: &mut [Item], dx: f32, dy: f32) {
    for it in items {
        match it {
            Item::Text { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Item::Line { from, to, .. } => {
                from.0 += dx;
                from.1 += dy;
                to.0 += dx;
                to.1 += dy;
            }
            Item::Path { points, .. } => {
                for p in points {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
        }
    }
}

fn empty() -> B {
    B { w: 0.0, asc: 0.0, desc: 0.0, items: Vec::new() }
}

fn text_box(text: &str, size: f32, italic: bool, m: &dyn Metrics) -> B {
    if text.is_empty() {
        return empty();
    }
    let w = m.width(text, size, italic).max(0.0);
    B { w, asc: ASC * size, desc: DESC * size, items: vec![Item::Text { x: 0.0, y: 0.0, text: text.to_string(), size, italic }] }
}

/// Lays out `node` at font size `size`.
pub fn layout(node: &Node, size: f32, m: &dyn Metrics) -> Layout {
    let size = if size.is_finite() && size > 1.0 { size.min(400.0) } else { 14.0 };
    let b = build(node, size, m, 0);
    let mut items = b.items;
    shift(&mut items, 0.0, b.asc);
    Layout { width: b.w, ascent: b.asc, descent: b.desc, items }
}

fn build(node: &Node, size: f32, m: &dyn Metrics, depth: usize) -> B {
    if depth > MAX_DEPTH {
        return empty();
    }
    match node {
        Node::Row(children) => row(children, size, m, depth),
        Node::Ident(s) => text_box(s, size, true, m),
        Node::Number(s) | Node::Text(s) => text_box(s, size, false, m),
        Node::Op { text, .. } => text_box(text, size, false, m),
        Node::Space(s) => B { w: s * size, asc: 0.0, desc: 0.0, items: Vec::new() },
        Node::Frac(n, d) => frac(n, d, size, m, depth),
        Node::Sqrt { index, body } => sqrt(index.as_deref(), body, size, m, depth),
        Node::Script { base, sub, sup } => script(base, sub.as_deref(), sup.as_deref(), size, m, depth),
        Node::Big { kind, sub, sup } => big(*kind, sub.as_deref(), sup.as_deref(), size, m, depth),
        Node::Delim { open, close, body } => delim(open, close, body, size, m, depth),
    }
}

fn row(children: &[Node], size: f32, m: &dyn Metrics, depth: usize) -> B {
    let mut out = empty();
    let mut x = 0.0;
    let mut prev_was_operator = true;
    for (i, c) in children.iter().enumerate() {
        let binary = matches!(c, Node::Op { binary: true, .. });
        // Space around a binary operator, but not when it is a sign (first, or after another operator).
        let gap = if binary && !prev_was_operator && i + 1 < children.len() { 0.22 * size } else { 0.0 };
        x += gap;
        let mut b = build(c, size, m, depth + 1);
        shift(&mut b.items, x, 0.0);
        out.items.append(&mut b.items);
        x += b.w + gap;
        out.asc = out.asc.max(b.asc);
        out.desc = out.desc.max(b.desc);
        prev_was_operator = binary || matches!(c, Node::Space(_));
        if matches!(c, Node::Op { text, binary: false } if text == ",") {
            x += 0.12 * size;
        }
    }
    out.w = x;
    out
}

fn frac(n: &Node, d: &Node, size: f32, m: &dyn Metrics, depth: usize) -> B {
    let fs = size * 0.92;
    let mut nb = build(n, fs, m, depth + 1);
    let mut db = build(d, fs, m, depth + 1);
    let pad = 0.14 * size;
    let w = nb.w.max(db.w) + 2.0 * pad;
    let bar_y = -AXIS * size;
    let gap = 0.12 * size;
    let num_y = bar_y - gap - nb.desc;
    let den_y = bar_y + gap + db.asc;
    shift(&mut nb.items, (w - nb.w) / 2.0, num_y);
    shift(&mut db.items, (w - db.w) / 2.0, den_y);
    let mut items = nb.items;
    items.append(&mut db.items);
    items.push(Item::Line { from: (pad * 0.5, bar_y), to: (w - pad * 0.5, bar_y), width: (0.065 * size).max(1.0) });
    B { w, asc: (-num_y + nb.asc).max(ASC * size), desc: (den_y + db.desc).max(DESC * size), items }
}

fn sqrt(index: Option<&Node>, body: &Node, size: f32, m: &dyn Metrics, depth: usize) -> B {
    let mut bb = build(body, size, m, depth + 1);
    let pad = 0.12 * size;
    let top = -(bb.asc + pad);
    let bottom = bb.desc;
    let rw = 0.55 * size;
    let y_mid = (top + bottom) / 2.0 + 0.12 * size;
    let (mut ib, ix) = match index {
        Some(i) => {
            let ib = build(i, size * 0.55, m, depth + 1);
            let ix = (ib.w - 0.4 * rw).max(0.0);
            (Some(ib), ix)
        }
        None => (None, 0.0),
    };
    let body_x = ix + rw + 0.06 * size;
    let end_x = body_x + bb.w + 0.08 * size;
    shift(&mut bb.items, body_x, 0.0);
    let mut items = Vec::new();
    items.push(Item::Path {
        points: vec![(ix, y_mid), (ix + 0.22 * rw, y_mid + 0.07 * size), (ix + 0.5 * rw, bottom), (ix + rw, top), (end_x, top)],
        width: (0.07 * size).max(1.0),
    });
    items.append(&mut bb.items);
    let mut asc = (-top + 0.04 * size).max(ASC * size);
    if let Some(ib) = ib.as_mut() {
        let iy = y_mid - 0.18 * size;
        shift(&mut ib.items, 0.0, iy);
        asc = asc.max(-iy + ib.asc);
        items.append(&mut ib.items);
    }
    B { w: end_x, asc, desc: bottom.max(DESC * size), items }
}

fn script(base: &Node, sub: Option<&Node>, sup: Option<&Node>, size: f32, m: &dyn Metrics, depth: usize) -> B {
    let bb = build(base, size, m, depth + 1);
    let ss = size * SCRIPT;
    let mut sup_b = sup.map(|n| build(n, ss, m, depth + 1));
    let mut sub_b = sub.map(|n| build(n, ss, m, depth + 1));
    let x = bb.w + 0.04 * size;
    let sup_y = -(bb.asc * 0.55).max(0.38 * size);
    let sub_y = (bb.desc * 0.8).max(0.2 * size) + 0.12 * size;
    let mut out = B { w: x, asc: bb.asc, desc: bb.desc, items: bb.items };
    let mut right: f32 = 0.0;
    if let Some(b) = sup_b.as_mut() {
        shift(&mut b.items, x, sup_y);
        out.asc = out.asc.max(-sup_y + b.asc);
        right = right.max(b.w);
        out.items.append(&mut b.items);
    }
    if let Some(b) = sub_b.as_mut() {
        shift(&mut b.items, x, sub_y);
        out.desc = out.desc.max(sub_y + b.desc);
        right = right.max(b.w);
        out.items.append(&mut b.items);
    }
    out.w = x + right;
    out
}

fn big(kind: BigKind, sub: Option<&Node>, sup: Option<&Node>, size: f32, m: &dyn Metrics, depth: usize) -> B {
    let h = 1.25 * size;
    let cy = -AXIS * size;
    let stroke = (0.075 * size).max(1.0);
    let (gw, mut items): (f32, Vec<Item>) = match kind {
        BigKind::Sum => {
            let w = 0.85 * size;
            (
                w,
                vec![Item::Path {
                    points: vec![(w * 0.95, cy - h / 2.0), (0.0, cy - h / 2.0), (w * 0.55, cy), (0.0, cy + h / 2.0), (w * 0.95, cy + h / 2.0)],
                    width: stroke,
                }],
            )
        }
        BigKind::Prod => {
            let w = 0.85 * size;
            (
                w,
                vec![
                    Item::Line { from: (0.0, cy - h / 2.0), to: (w, cy - h / 2.0), width: stroke },
                    Item::Line { from: (w * 0.18, cy - h / 2.0), to: (w * 0.18, cy + h / 2.0), width: stroke },
                    Item::Line { from: (w * 0.82, cy - h / 2.0), to: (w * 0.82, cy + h / 2.0), width: stroke },
                ],
            )
        }
        BigKind::Int => {
            let w = 0.55 * size;
            let pts = [
                (0.45, -0.40),
                (0.38, -0.48),
                (0.28, -0.50),
                (0.18, -0.42),
                (0.08, -0.2),
                (0.0, 0.2),
                (-0.08, 0.42),
                (-0.18, 0.50),
                (-0.28, 0.48),
                (-0.35, 0.40),
            ];
            (w, vec![Item::Path { points: pts.iter().map(|&(x, y)| ((x + 0.5) * w, cy + y * h)).collect(), width: stroke }])
        }
    };
    let ss = size * SCRIPT;
    let mut sup_b = sup.map(|n| build(n, ss, m, depth + 1));
    let mut sub_b = sub.map(|n| build(n, ss, m, depth + 1));
    let gap = 0.08 * size;
    let (mut asc, mut desc) = (-(cy - h / 2.0), cy + h / 2.0);
    let width;
    if kind == BigKind::Int {
        let x = gw + 0.02 * size;
        let mut right: f32 = 0.0;
        if let Some(b) = sup_b.as_mut() {
            let y = cy - h / 2.0 + 0.32 * size;
            shift(&mut b.items, x + 0.18 * size, y);
            asc = asc.max(-y + b.asc);
            right = right.max(b.w + 0.18 * size);
            items.append(&mut b.items);
        }
        if let Some(b) = sub_b.as_mut() {
            let y = cy + h / 2.0 - 0.02 * size;
            shift(&mut b.items, x - 0.12 * size, y);
            desc = desc.max(y + b.desc);
            right = right.max(b.w - 0.12 * size);
            items.append(&mut b.items);
        }
        width = x + right.max(0.0);
    } else {
        let w = gw.max(sup_b.as_ref().map(|b| b.w).unwrap_or(0.0)).max(sub_b.as_ref().map(|b| b.w).unwrap_or(0.0));
        shift(&mut items, (w - gw) / 2.0, 0.0);
        if let Some(b) = sup_b.as_mut() {
            let y = cy - h / 2.0 - gap - b.desc;
            shift(&mut b.items, (w - b.w) / 2.0, y);
            asc = asc.max(-y + b.asc);
            items.append(&mut b.items);
        }
        if let Some(b) = sub_b.as_mut() {
            let y = cy + h / 2.0 + gap + b.asc;
            shift(&mut b.items, (w - b.w) / 2.0, y);
            desc = desc.max(y + b.desc);
            items.append(&mut b.items);
        }
        width = w;
    }
    B { w: width + 0.1 * size, asc, desc, items }
}

fn delim(open: &str, close: &str, body: &Node, size: f32, m: &dyn Metrics, depth: usize) -> B {
    let mut bb = build(body, size, m, depth + 1);
    let h = (bb.asc + bb.desc).max(size);
    let ds = (h * 1.08).min(size * 6.0);
    let center = (bb.desc - bb.asc) / 2.0;
    let base_y = center + (ASC - 0.5) * ds + 0.0;
    let mut out = empty();
    let mut x = 0.0;
    if !open.is_empty() {
        let mut ob = text_box(open, ds, false, m);
        shift(&mut ob.items, x, base_y);
        x += ob.w;
        out.items.append(&mut ob.items);
    }
    shift(&mut bb.items, x, 0.0);
    x += bb.w;
    out.items.append(&mut bb.items);
    if !close.is_empty() {
        let mut cb = text_box(close, ds, false, m);
        shift(&mut cb.items, x, base_y);
        x += cb.w;
        out.items.append(&mut cb.items);
    }
    out.w = x;
    out.asc = (-(center - h / 2.0 * 1.04)).max(bb.asc);
    out.desc = (center + h / 2.0 * 1.04).max(bb.desc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    /// Every character is half the font size wide.
    struct Mono;
    impl Metrics for Mono {
        fn width(&self, text: &str, size: f32, _italic: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    fn lay(src: &str) -> Layout {
        layout(&parse(src), 20.0, &Mono)
    }

    fn texts(l: &Layout) -> Vec<(&str, f32, f32, f32)> {
        l.items
            .iter()
            .filter_map(|i| match i {
                Item::Text { x, y, text, size, .. } => Some((text.as_str(), *x, *y, *size)),
                _ => None,
            })
            .collect()
    }

    fn within(l: &Layout) {
        let tol = 1.0;
        for it in &l.items {
            let pts: Vec<(f32, f32)> = match it {
                Item::Text { x, y, .. } => vec![(*x, *y)],
                Item::Line { from, to, .. } => vec![*from, *to],
                Item::Path { points, .. } => points.clone(),
            };
            for (x, y) in pts {
                assert!(x.is_finite() && y.is_finite());
                assert!(x >= -tol && x <= l.width + tol, "x {x} outside 0..{}", l.width);
                assert!(y >= -tol && y <= l.height() + tol, "y {y} outside 0..{}", l.height());
            }
        }
    }

    #[test]
    fn a_letter_has_the_expected_box() {
        let l = lay("x");
        assert_eq!(l.width, 10.0);
        assert!((l.ascent - 0.78 * 20.0).abs() < 0.01 && (l.descent - 0.22 * 20.0).abs() < 0.01);
        assert_eq!(texts(&l), vec![("x", 0.0, l.ascent, 20.0)]);
    }

    #[test]
    fn binary_operators_get_space_but_signs_do_not() {
        let spaced = lay("a+b");
        let tight = lay("ab");
        assert!(spaced.width > tight.width + 10.0, "a plus sign adds its own width and space");
        let sign = lay("-x");
        let plain = lay("x");
        assert!((sign.width - plain.width - 10.0).abs() < 0.01, "a leading minus is a sign: no extra space");
    }

    #[test]
    fn a_fraction_stacks_numerator_over_denominator_with_a_bar() {
        let l = lay("\\frac{a}{b}");
        let t = texts(&l);
        let (na, nb) = (t.iter().find(|x| x.0 == "a").expect("a"), t.iter().find(|x| x.0 == "b").expect("b"));
        assert!(na.2 < nb.2, "the numerator is above the denominator");
        let bar = l.items.iter().find_map(|i| if let Item::Line { from, to, .. } = i { Some((from.1, from.0, to.0)) } else { None }).expect("bar");
        assert!(na.2 < bar.0 && bar.0 < nb.2, "the bar is between them");
        assert!(bar.2 - bar.1 > 0.0);
        within(&l);
    }

    #[test]
    fn scripts_sit_above_and_below_the_baseline() {
        let l = lay("x^2_i");
        let t = texts(&l);
        let base = t.iter().find(|x| x.0 == "x").expect("x");
        let sup = t.iter().find(|x| x.0 == "2").expect("2");
        let sub = t.iter().find(|x| x.0 == "i").expect("i");
        assert!(sup.2 < base.2 && sub.2 > base.2);
        assert!(sup.3 < base.3 && sub.3 < base.3, "scripts are smaller");
        assert!(sup.1 > base.1 && sub.1 > base.1, "and to the right");
        within(&l);
    }

    #[test]
    fn nested_scripts_shrink_again() {
        let l = lay("e^{x^2}");
        let sizes: Vec<f32> = texts(&l).iter().map(|t| t.3).collect();
        assert!(sizes.windows(2).any(|w| w[1] < w[0]));
        within(&l);
    }

    #[test]
    fn a_root_draws_a_radical_path_that_ends_in_a_bar_over_the_body() {
        let l = lay("\\sqrt{x+1}");
        let path = l.items.iter().find_map(|i| if let Item::Path { points, .. } = i { Some(points.clone()) } else { None }).expect("radical");
        assert_eq!(path.len(), 5);
        let (last, before) = (path[4], path[3]);
        assert_eq!(last.1, before.1, "the last segment is horizontal");
        assert!(last.0 > before.0 + 10.0, "and extends over the body");
        within(&l);
        let indexed = lay("\\sqrt[3]{x}");
        assert!(indexed.width > lay("\\sqrt{x}").width - 0.01 && texts(&indexed).iter().any(|t| t.0 == "3" && t.3 < 20.0));
    }

    #[test]
    fn a_sum_puts_its_limits_above_and_below() {
        let l = lay("\\sum_{i=1}^{n} i");
        let t = texts(&l);
        let n = t.iter().find(|x| x.0 == "n").expect("n");
        let one = t.iter().find(|x| x.0 == "1").expect("1");
        assert!(n.2 < one.2, "the upper limit is above the lower limit");
        assert!(l.items.iter().any(|i| matches!(i, Item::Path { .. })), "the sum sign is a path");
        within(&l);
    }

    #[test]
    fn an_integral_puts_its_limits_to_the_side() {
        let l = lay("\\int_0^1 x\\,dx");
        let t = texts(&l);
        let zero = t.iter().find(|x| x.0 == "0").expect("0");
        let x = t.iter().find(|x| x.0 == "x").expect("x");
        assert!(zero.1 < x.1, "the limits come before the integrand");
        within(&l);
    }

    #[test]
    fn products_draw_a_bar_and_two_legs() {
        let l = lay("\\prod_{k} k");
        assert_eq!(l.items.iter().filter(|i| matches!(i, Item::Line { .. })).count(), 3);
        within(&l);
    }

    #[test]
    fn delimiters_grow_with_a_tall_body() {
        let short = lay("\\left( x \\right)");
        let tall = lay("\\left( \\frac{a}{b} \\right)");
        let size_of = |l: &Layout| texts(l).iter().find(|t| t.0 == "(").map(|t| t.3).expect("open");
        assert!(size_of(&tall) > size_of(&short));
        within(&tall);
    }

    #[test]
    fn spacing_commands_add_width() {
        assert!(lay("a\\quad b").width > lay("ab").width + 15.0);
        assert!(lay("a\\,b").width > lay("ab").width);
    }

    #[test]
    fn a_larger_font_scales_the_whole_formula() {
        let a = layout(&parse("\\frac{x^2}{y}"), 10.0, &Mono);
        let b = layout(&parse("\\frac{x^2}{y}"), 20.0, &Mono);
        assert!((b.width / a.width - 2.0).abs() < 0.01);
        assert!((b.height() / a.height() - 2.0).abs() < 0.01);
    }

    #[test]
    fn a_famous_formula_lays_out_inside_its_box() {
        for src in [
            "E = mc^2",
            "\\int_0^1 x^2\\,dx = \\tfrac{1}{3}",
            "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
            "\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}",
            "e^{i\\pi} + 1 = 0",
            "\\left( \\frac{a}{b} \\right)^2",
            "\\alpha + \\beta \\leq \\gamma",
        ] {
            let l = lay(src);
            assert!(l.width > 0.0 && l.height() > 0.0, "{src}");
            within(&l);
        }
    }

    #[test]
    fn odd_sizes_and_hostile_input_are_safe() {
        for size in [0.0, -5.0, f32::NAN, f32::INFINITY, 1e9] {
            let l = layout(&parse("\\frac{x}{y}"), size, &Mono);
            assert!(l.width.is_finite() && l.height().is_finite());
        }
        for src in ["", "{", "\\frac", "\\sqrt[", &"\\frac{".repeat(200), &"x^{".repeat(200), &"\\left(".repeat(100)] {
            let l = lay(src);
            assert!(l.width.is_finite() && l.height().is_finite());
        }
    }
}
