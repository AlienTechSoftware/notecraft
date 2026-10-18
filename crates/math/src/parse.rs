//! TeX subset to a tree. Never fails; unknown input stays as text.

/// The large operators that take limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BigKind {
    Sum,
    Prod,
    Int,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Row(Vec<Node>),
    /// A variable: one italic letter or symbol.
    Ident(String),
    Number(String),
    /// An upright symbol or operator. `binary` operators get space around them.
    Op {
        text: String,
        binary: bool,
    },
    /// Upright text (`\text`, function names).
    Text(String),
    Frac(Box<Node>, Box<Node>),
    Sqrt {
        index: Option<Box<Node>>,
        body: Box<Node>,
    },
    Script {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Big {
        kind: BigKind,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Delim {
        open: String,
        close: String,
        body: Box<Node>,
    },
    /// Horizontal space in units of the font size.
    Space(f32),
}

const MAX_LEN: usize = 4000;
const MAX_DEPTH: usize = 48;

pub fn parse(src: &str) -> Node {
    let chars: Vec<char> = src.chars().take(MAX_LEN).collect();
    let mut p = Parser { chars, pos: 0, depth: 0 };
    let nodes = p.row(None);
    Node::Row(nodes)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

fn greek(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "\u{3b1}",
        "beta" => "\u{3b2}",
        "gamma" => "\u{3b3}",
        "delta" => "\u{3b4}",
        "epsilon" | "varepsilon" => "\u{3b5}",
        "zeta" => "\u{3b6}",
        "eta" => "\u{3b7}",
        "theta" | "vartheta" => "\u{3b8}",
        "iota" => "\u{3b9}",
        "kappa" => "\u{3ba}",
        "lambda" => "\u{3bb}",
        "mu" => "\u{3bc}",
        "nu" => "\u{3bd}",
        "xi" => "\u{3be}",
        "pi" => "\u{3c0}",
        "rho" => "\u{3c1}",
        "sigma" => "\u{3c3}",
        "tau" => "\u{3c4}",
        "upsilon" => "\u{3c5}",
        "phi" | "varphi" => "\u{3c6}",
        "chi" => "\u{3c7}",
        "psi" => "\u{3c8}",
        "omega" => "\u{3c9}",
        "Gamma" => "\u{393}",
        "Delta" => "\u{394}",
        "Theta" => "\u{398}",
        "Lambda" => "\u{39b}",
        "Xi" => "\u{39e}",
        "Pi" => "\u{3a0}",
        "Sigma" => "\u{3a3}",
        "Phi" => "\u{3a6}",
        "Psi" => "\u{3a8}",
        "Omega" => "\u{3a9}",
        _ => return None,
    })
}

/// Symbols that are operators or relations: (text, binary).
fn symbol(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "pm" => ("\u{b1}", true),
        "mp" => ("\u{2213}", true),
        "times" => ("\u{d7}", true),
        "div" => ("\u{f7}", true),
        "cdot" => ("\u{b7}", true),
        "leq" | "le" => ("\u{2264}", true),
        "geq" | "ge" => ("\u{2265}", true),
        "neq" | "ne" => ("\u{2260}", true),
        "approx" => ("\u{2248}", true),
        "equiv" => ("\u{2261}", true),
        "sim" => ("~", true),
        "in" => ("\u{2208}", true),
        "notin" => ("\u{2209}", true),
        "subset" => ("\u{2282}", true),
        "cup" => ("\u{222a}", true),
        "cap" => ("\u{2229}", true),
        "to" | "rightarrow" => ("\u{2192}", true),
        "leftarrow" => ("\u{2190}", true),
        "Rightarrow" | "implies" => ("\u{21d2}", true),
        "leftrightarrow" => ("\u{2194}", true),
        "infty" => ("\u{221e}", false),
        "partial" => ("\u{2202}", false),
        "nabla" => ("\u{2207}", false),
        "forall" => ("\u{2200}", false),
        "exists" => ("\u{2203}", false),
        "ldots" | "dots" | "cdots" => ("\u{2026}", false),
        "degree" => ("\u{b0}", false),
        _ => return None,
    })
}

fn function_name(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "log"
            | "ln"
            | "exp"
            | "lim"
            | "min"
            | "max"
            | "det"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
    )
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    /// Parses items until `end` (a closing brace, or `\right`) or the end of the input.
    fn row(&mut self, end: Option<char>) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        loop {
            self.skip_space();
            let Some(c) = self.peek() else { break };
            if Some(c) == end {
                break;
            }
            if c == '\\' && self.at_command("right") {
                break;
            }
            let Some(atom) = self.atom() else { continue };
            out.push(self.scripts(atom));
        }
        out
    }

    fn at_command(&self, name: &str) -> bool {
        let want: Vec<char> = std::iter::once('\\').chain(name.chars()).collect();
        self.chars.get(self.pos..self.pos + want.len()) == Some(want.as_slice())
            && !self.chars.get(self.pos + want.len()).is_some_and(|c| c.is_ascii_alphabetic())
    }

    /// Applies any `^` and `_` that follow `base`.
    fn scripts(&mut self, base: Node) -> Node {
        let (mut sub, mut sup) = (None, None);
        loop {
            self.skip_space();
            match self.peek() {
                Some('^') if sup.is_none() => {
                    self.pos += 1;
                    sup = Some(Box::new(self.operand()));
                }
                Some('_') if sub.is_none() => {
                    self.pos += 1;
                    sub = Some(Box::new(self.operand()));
                }
                _ => break,
            }
        }
        if sub.is_none() && sup.is_none() {
            return base;
        }
        match base {
            Node::Big { kind, sub: None, sup: None } => Node::Big { kind, sub, sup },
            base => Node::Script { base: Box::new(base), sub, sup },
        }
    }

    /// The argument of `^`, `_` or a command: a group, or one token.
    fn operand(&mut self) -> Node {
        self.skip_space();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                self.group_body()
            }
            Some(_) => match self.atom() {
                Some(n) => n,
                None => Node::Row(Vec::new()),
            },
            None => Node::Row(Vec::new()),
        }
    }

    /// After an opening brace: the contents, consuming the closing brace.
    fn group_body(&mut self) -> Node {
        if self.depth >= MAX_DEPTH {
            return Node::Text(String::new());
        }
        self.depth += 1;
        let nodes = self.row(Some('}'));
        self.depth -= 1;
        if self.peek() == Some('}') {
            self.pos += 1;
        }
        Node::Row(nodes)
    }

    fn raw_group(&mut self) -> String {
        self.skip_space();
        if self.peek() != Some('{') {
            return self
                .peek()
                .map(|c| {
                    self.pos += 1;
                    c.to_string()
                })
                .unwrap_or_default();
        }
        self.pos += 1;
        let mut depth = 1;
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '{' => {
                    depth += 1;
                    s.push(c);
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    s.push(c);
                }
                _ => s.push(c),
            }
        }
        s
    }

    fn atom(&mut self) -> Option<Node> {
        let c = self.peek()?;
        self.pos += 1;
        Some(match c {
            '{' => self.group_body(),
            '\\' => self.command(),
            '}' | '^' | '_' => Node::Text(c.to_string()),
            c if c.is_ascii_digit() || (c == '.' && self.peek().is_some_and(|n| n.is_ascii_digit())) => {
                let mut s = c.to_string();
                while let Some(n) = self.peek() {
                    if n.is_ascii_digit() || n == '.' {
                        s.push(n);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                Node::Number(s)
            }
            c if c.is_alphabetic() => Node::Ident(c.to_string()),
            '+' | '=' | '<' | '>' => Node::Op { text: c.to_string(), binary: true },
            '-' => Node::Op { text: "\u{2212}".to_string(), binary: true },
            '*' => Node::Op { text: "\u{2217}".to_string(), binary: true },
            ',' | ';' => Node::Op { text: c.to_string(), binary: false },
            c => Node::Op { text: c.to_string(), binary: false },
        })
    }

    fn command(&mut self) -> Node {
        let Some(first) = self.peek() else {
            return Node::Text("\\".into());
        };
        if !first.is_ascii_alphabetic() {
            self.pos += 1;
            return match first {
                ',' => Node::Space(0.17),
                ':' | ';' => Node::Space(0.25),
                '!' => Node::Space(-0.1),
                ' ' => Node::Space(0.3),
                '{' | '}' | '|' | '%' | '$' | '&' | '#' | '_' => Node::Op { text: first.to_string(), binary: false },
                '\\' => Node::Space(0.5),
                c => Node::Text(format!("\\{c}")),
            };
        }
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                name.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        if let Some(g) = greek(&name) {
            return Node::Ident(g.to_string());
        }
        if let Some((text, binary)) = symbol(&name) {
            return Node::Op { text: text.to_string(), binary };
        }
        if function_name(&name) {
            return Node::Text(name);
        }
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.operand();
                let b = self.operand();
                Node::Frac(Box::new(a), Box::new(b))
            }
            "sqrt" => {
                self.skip_space();
                let index = if self.peek() == Some('[') {
                    self.pos += 1;
                    let mut s = String::new();
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == ']' {
                            break;
                        }
                        s.push(c);
                    }
                    Some(Box::new(parse(&s)))
                } else {
                    None
                };
                let body = self.operand();
                Node::Sqrt { index, body: Box::new(body) }
            }
            "sum" => Node::Big { kind: BigKind::Sum, sub: None, sup: None },
            "prod" => Node::Big { kind: BigKind::Prod, sub: None, sup: None },
            "int" => Node::Big { kind: BigKind::Int, sub: None, sup: None },
            "text" | "mathrm" | "textrm" | "mathbf" | "mathbb" | "mathcal" | "operatorname" | "mbox" => Node::Text(self.raw_group()),
            "left" => {
                let open = self.delimiter();
                if self.depth >= MAX_DEPTH {
                    return Node::Text(String::new());
                }
                self.depth += 1;
                let body = self.row(None);
                self.depth -= 1;
                let close = if self.at_command("right") {
                    self.pos += "\\right".chars().count();
                    self.delimiter()
                } else {
                    String::new()
                };
                Node::Delim { open, close, body: Box::new(Node::Row(body)) }
            }
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            other => Node::Text(format!("\\{other}")),
        }
    }

    fn delimiter(&mut self) -> String {
        self.skip_space();
        match self.peek() {
            Some('.') => {
                self.pos += 1;
                String::new()
            }
            Some('\\') => {
                self.pos += 1;
                match self.peek() {
                    Some(c) if !c.is_ascii_alphabetic() => {
                        self.pos += 1;
                        c.to_string()
                    }
                    _ => {
                        let mut name = String::new();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_alphabetic() {
                                name.push(c);
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                        match name.as_str() {
                            "langle" => "\u{27e8}".into(),
                            "rangle" => "\u{27e9}".into(),
                            "lvert" | "rvert" | "vert" => "|".into(),
                            _ => String::new(),
                        }
                    }
                }
            }
            Some(c) => {
                self.pos += 1;
                c.to_string()
            }
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(src: &str) -> Vec<Node> {
        match parse(src) {
            Node::Row(v) => v,
            other => vec![other],
        }
    }

    #[test]
    fn letters_numbers_and_operators() {
        assert_eq!(row("x+2"), vec![Node::Ident("x".into()), Node::Op { text: "+".into(), binary: true }, Node::Number("2".into())]);
        assert_eq!(row("3.14"), vec![Node::Number("3.14".into())]);
        assert_eq!(row("a-b")[1], Node::Op { text: "\u{2212}".into(), binary: true });
    }

    #[test]
    fn scripts_attach_to_the_previous_atom() {
        match &row("x^2_i")[0] {
            Node::Script { base, sub, sup } => {
                assert_eq!(**base, Node::Ident("x".into()));
                assert!(sub.is_some() && sup.is_some());
            }
            other => panic!("{other:?}"),
        }
        match &row("e^{-x^2}")[0] {
            Node::Script { sup: Some(s), .. } => assert!(matches!(**s, Node::Row(_))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fractions_roots_and_big_operators() {
        assert!(matches!(&row("\\frac{1}{3}")[0], Node::Frac(_, _)));
        assert!(matches!(&row("\\tfrac12")[0], Node::Frac(_, _)), "single-token operands");
        assert!(matches!(&row("\\sqrt{x}")[0], Node::Sqrt { index: None, .. }));
        assert!(matches!(&row("\\sqrt[3]{x}")[0], Node::Sqrt { index: Some(_), .. }));
        match &row("\\int_0^1 x\\,dx")[0] {
            Node::Big { kind: BigKind::Int, sub: Some(_), sup: Some(_) } => {}
            other => panic!("{other:?}"),
        }
        assert!(matches!(&row("\\sum_{i=1}^{n} i")[0], Node::Big { kind: BigKind::Sum, .. }));
    }

    #[test]
    fn greek_symbols_and_functions() {
        assert_eq!(row("\\alpha"), vec![Node::Ident("\u{3b1}".into())]);
        assert_eq!(row("\\Omega"), vec![Node::Ident("\u{3a9}".into())]);
        assert_eq!(row("\\leq"), vec![Node::Op { text: "\u{2264}".into(), binary: true }]);
        assert_eq!(row("\\infty"), vec![Node::Op { text: "\u{221e}".into(), binary: false }]);
        assert_eq!(row("\\sin"), vec![Node::Text("sin".into())]);
    }

    #[test]
    fn text_delimiters_and_spacing() {
        assert_eq!(row("\\text{if } x"), vec![Node::Text("if ".into()), Node::Ident("x".into())]);
        assert!(matches!(&row("\\left( x \\right)")[0], Node::Delim { open, close, .. } if open == "(" && close == ")"));
        assert!(matches!(&row("\\left\\langle x \\right.")[0], Node::Delim { open, close, .. } if open == "\u{27e8}" && close.is_empty()));
        assert_eq!(row("a\\,b")[1], Node::Space(0.17));
        assert_eq!(row("\\quad")[0], Node::Space(1.0));
    }

    #[test]
    fn unknown_input_is_kept_as_text() {
        assert_eq!(row("\\unknowncmd"), vec![Node::Text("\\unknowncmd".into())]);
        assert_eq!(row("\\mathbb{R}"), vec![Node::Text("R".into())]);
    }

    #[test]
    fn hostile_input_never_panics() {
        let samples = [
            "",
            "{",
            "}",
            "^",
            "_",
            "\\",
            "\\frac",
            "\\frac{1}",
            "\\sqrt[",
            "\\sqrt[3",
            "\\left",
            "\\left(",
            "\\right)",
            "\\text{",
            "x^",
            "x_^",
            "{{{{{{{{",
            &"{".repeat(500),
            &"^".repeat(500),
            &"\\frac".repeat(300),
            &"\\left(".repeat(300),
            &"x^{".repeat(300),
            "\u{0}\u{1}\u{fffd}",
            "\\\u{e9}",
            "$$",
            "\\,\\,\\,",
        ];
        for s in samples {
            let _ = parse(s);
        }
    }

    #[test]
    fn long_input_is_cut() {
        let long = "x".repeat(MAX_LEN * 3);
        match parse(&long) {
            Node::Row(v) => assert_eq!(v.len(), MAX_LEN),
            other => panic!("{other:?}"),
        }
    }
}
