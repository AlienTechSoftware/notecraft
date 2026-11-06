use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, File, ItemFn, LitStr, Macro};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Problem {
    line: usize,
    message: String,
}

#[derive(Default)]
struct Catalogs {
    english: BTreeMap<String, String>,
    english_plurals: BTreeSet<String>,
    locales: BTreeMap<String, BTreeMap<String, String>>,
    locale_plurals: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Default)]
struct HelperCollector {
    parameters: BTreeMap<String, BTreeSet<usize>>,
    directives: BTreeSet<usize>,
    problems: Vec<String>,
    lines: Vec<String>,
}

impl HelperCollector {
    fn inspect(&mut self, name: &str, inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>, line: usize) {
        let Some((directive_line, names)) = annotation_before(&self.lines, line, "i18n-ui-text:") else { return };
        self.directives.insert(directive_line);
        let names: Vec<_> = names.split(',').map(str::trim).filter(|name| !name.is_empty()).collect();
        if names.is_empty() {
            self.problems.push(format!("line {directive_line}: i18n-ui-text annotation must name one or more text parameters"));
            return;
        }
        let parameters: Vec<_> = inputs
            .iter()
            .filter_map(|input| match input {
                syn::FnArg::Typed(typed) => match typed.pat.as_ref() {
                    syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
                    _ => None,
                },
                syn::FnArg::Receiver(_) => None,
            })
            .collect();
        let mut indices = BTreeSet::new();
        for parameter in names {
            if let Some(index) = parameters.iter().position(|actual| actual == parameter) {
                indices.insert(index);
            } else {
                self.problems.push(format!("line {directive_line}: {name} has no parameter named {parameter:?}"));
            }
        }
        if !indices.is_empty() {
            self.parameters.entry(name.to_string()).or_default().extend(indices);
        }
    }
}

impl<'ast> Visit<'ast> for HelperCollector {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.inspect(&item.sig.ident.to_string(), &item.sig.inputs, item.sig.ident.span().start().line);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.inspect(&item.sig.ident.to_string(), &item.sig.inputs, item.sig.ident.span().start().line);
        visit::visit_impl_item_fn(self, item);
    }
}

fn annotation_before<'a>(lines: &'a [String], line: usize, tag: &str) -> Option<(usize, &'a str)> {
    let mut cursor = line.saturating_sub(1);
    while cursor > 0 {
        cursor -= 1;
        let trimmed = lines.get(cursor)?.trim();
        if let Some(value) = trimmed.strip_prefix("//").map(str::trim).and_then(|comment| comment.strip_prefix(tag)) {
            return Some((cursor + 1, value.trim()));
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("# [") || trimmed.starts_with("#[") {
            continue;
        }
        break;
    }
    None
}

#[derive(Default)]
struct LiteralCollector {
    literals: Vec<LitStr>,
}

impl<'ast> Visit<'ast> for LiteralCollector {
    fn visit_macro(&mut self, mac: &'ast Macro) {
        if macro_name(mac).is_some_and(|name| matches!(name.as_str(), "tl_id" | "tl")) {
            return;
        }
        visit::visit_macro(self, mac);
    }

    fn visit_lit_str(&mut self, literal: &'ast LitStr) {
        self.literals.push(literal.clone());
    }

    fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
        if let Expr::Path(path) = expression.func.as_ref()
            && path.path.segments.last().is_some_and(|segment| {
                matches!(segment.ident.to_string().as_str(), "format_message_id" | "format_count_id" | "format_count_message_id")
            })
        {
            return;
        }
        visit::visit_expr_call(self, expression);
    }
}

fn macro_name(mac: &Macro) -> Option<String> {
    mac.path.segments.last().map(|segment| segment.ident.to_string())
}

fn is_catalog_lookup(expr: &Expr) -> bool {
    match expr {
        Expr::Macro(mac) => macro_name(&mac.mac).is_some_and(|name| matches!(name.as_str(), "tl_id" | "tl")),
        Expr::Call(call) => match call.func.as_ref() {
            Expr::Path(path) => path.path.segments.last().is_some_and(|segment| {
                matches!(segment.ident.to_string().as_str(), "format_message_id" | "format_count_id" | "format_count_message_id")
            }),
            _ => false,
        },
        Expr::Paren(paren) => is_catalog_lookup(&paren.expr),
        Expr::Group(group) => is_catalog_lookup(&group.expr),
        _ => false,
    }
}

fn literal_line(span: Span) -> usize {
    span.start().line
}

struct SourceScanner<'a> {
    source_lines: Vec<String>,
    english_ids: &'a BTreeSet<String>,
    helpers: &'a BTreeMap<String, BTreeSet<usize>>,
    problems: BTreeSet<Problem>,
    used_exemptions: BTreeSet<usize>,
}

impl SourceScanner<'_> {
    fn report(&mut self, line: usize, message: impl Into<String>) {
        self.problems.insert(Problem { line, message: message.into() });
    }

    fn exemption(&mut self, line: usize) -> bool {
        let mut found = None;
        for candidate in [line, line.saturating_sub(1)] {
            let Some(text) = self.source_lines.get(candidate.saturating_sub(1)) else { continue };
            if let Some((_, reason)) = text.split_once("i18n-exempt:") {
                found = Some((candidate, reason.split("//").next().unwrap_or(reason).trim()));
                break;
            }
        }
        let Some((annotation_line, reason)) = found else { return false };
        self.used_exemptions.insert(annotation_line);
        if reason.is_empty() {
            self.report(annotation_line, "i18n-exempt comment requires a reason");
            return true;
        }
        true
    }

    fn raw_literals(&mut self, expr: &Expr, context: &str) {
        let mut collector = LiteralCollector::default();
        collector.visit_expr(expr);
        for literal in collector.literals {
            let line = literal_line(literal.span());
            if literal.value().is_empty() || self.exemption(line) {
                continue;
            }
            self.report(
                line,
                format!("hard-coded UI string in {context}: {:?}; use a stable `tl_id!` catalog ID or document an exemption", literal.value()),
            );
        }
    }

    fn check_id(&mut self, literal: &LitStr, kind: &str) {
        let id = literal.value();
        if !self.english_ids.contains(&id) {
            self.report(literal_line(literal.span()), format!("unknown UI ID {id:?} in {kind}; add it to i18n/en.tsv"));
        }
    }

    fn check_translation_macro(&mut self, mac: &Macro) {
        let Some(name) = macro_name(mac) else { return };
        if name == "tl_id" {
            match mac.parse_body::<LitStr>() {
                Ok(id) => self.check_id(&id, "tl_id!"),
                Err(_) => self.report(mac.span().start().line, "tl_id! must contain one literal stable ID"),
            }
        } else if name == "tl"
            && let Ok(literal) = mac.parse_body::<LitStr>()
        {
            self.report(literal_line(literal.span()), format!("hard-coded UI string in tl!: {:?}; use `tl_id!`", literal.value()));
        }
    }

    fn check_localized_call(&mut self, expression: &ExprCall) {
        let Expr::Path(path) = expression.func.as_ref() else { return };
        let Some(name) = path.path.segments.last().map(|segment| segment.ident.to_string()) else { return };
        if !matches!(name.as_str(), "format_message_id" | "format_count_id" | "format_count_message_id") {
            return;
        }
        if let Some(Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(id), .. })) = expression.args.first() {
            self.check_id(id, &name);
        }
    }

    fn check_helper_call<'a>(&mut self, name: &str, args: impl Iterator<Item = &'a Expr>) {
        let Some(indices) = self.helpers.get(name) else { return };
        let args: Vec<_> = args.collect();
        for index in indices {
            if let Some(argument) = args.get(*index)
                && !is_catalog_lookup(argument)
            {
                self.raw_literals(argument, &format!("annotated UI helper `{name}` argument {}", index + 1));
            }
        }
    }
}

impl<'ast> Visit<'ast> for SourceScanner<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item
            .attrs
            .iter()
            .any(|attr| matches!(&attr.meta, syn::Meta::List(list) if attr.path().is_ident("cfg") && list.tokens.to_string().contains("test")))
        {
            return;
        }
        visit::visit_item_mod(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if item
            .attrs
            .iter()
            .any(|attr| matches!(&attr.meta, syn::Meta::List(list) if attr.path().is_ident("cfg") && list.tokens.to_string().contains("test")))
        {
            return;
        }
        visit::visit_item_fn(self, item);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        self.check_translation_macro(mac);
        if macro_name(mac).is_some_and(|name| matches!(name.as_str(), "tl_id" | "tl")) {
            return;
        }
        visit::visit_macro(self, mac);
    }

    fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
        self.check_localized_call(expression);
        if let Expr::Path(path) = expression.func.as_ref()
            && let Some(name) = path.path.segments.last().map(|segment| segment.ident.to_string())
        {
            if name == "new"
                && path.path.segments.iter().any(|segment| segment.ident == "Button")
                && let Some(label) = expression.args.first()
                && !is_catalog_lookup(label)
            {
                self.raw_literals(label, "Button::new");
            }
            self.check_helper_call(&name, expression.args.iter());
        }
        visit::visit_expr_call(self, expression);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
        let name = expression.method.to_string();
        if matches!(
            name.as_str(),
            "label"
                | "button"
                | "small_button"
                | "selectable_label"
                | "checkbox"
                | "radio"
                | "hyperlink"
                | "on_hover_text"
                | "on_hover_text_at_pointer"
        ) && let Some(label) = expression.args.first()
            && !is_catalog_lookup(label)
        {
            self.raw_literals(label, &format!("UI method `{name}`"));
        }
        self.check_helper_call(&name, expression.args.iter());
        visit::visit_expr_method_call(self, expression);
    }
}

fn source_helpers(file: &File, source: &str) -> (BTreeMap<String, BTreeSet<usize>>, Vec<String>) {
    let mut collector = HelperCollector { lines: source.lines().map(str::to_string).collect(), ..HelperCollector::default() };
    collector.visit_file(file);
    for (line_index, line) in collector.lines.iter().enumerate() {
        if line.contains("i18n-ui-text:") && !collector.directives.contains(&(line_index + 1)) {
            collector.problems.push(format!("line {}: i18n-ui-text annotation is not attached to a function", line_index + 1));
        }
    }
    (collector.parameters, collector.problems)
}

fn scan_parsed_source(source: &str, file: &File, english_ids: &BTreeSet<String>, helpers: &BTreeMap<String, BTreeSet<usize>>) -> Vec<Problem> {
    let mut scanner = SourceScanner {
        source_lines: source.lines().map(str::to_string).collect(),
        english_ids,
        helpers,
        problems: BTreeSet::new(),
        used_exemptions: BTreeSet::new(),
    };
    scanner.visit_file(file);
    let unused: Vec<_> = scanner
        .source_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.contains("i18n-exempt:") && !scanner.used_exemptions.contains(&(index + 1))).then_some((index + 1, line.clone()))
        })
        .collect();
    for (line, text) in unused {
        let reason = text.split_once("i18n-exempt:").map_or("", |(_, reason)| reason.trim());
        let message = if reason.is_empty() { "i18n-exempt comment requires a reason" } else { "unused i18n-exempt comment" };
        scanner.report(line, message);
    }
    scanner.problems.into_iter().collect()
}

#[cfg(test)]
fn scan_source(path: &str, source: &str, english_ids: &BTreeSet<String>) -> Vec<Problem> {
    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(error) => return vec![Problem { line: error.span().start().line, message: format!("cannot parse {path}: {error}") }],
    };
    let (helpers, mut helper_problems) = source_helpers(&file, source);
    let mut problems = scan_parsed_source(source, &file, english_ids, &helpers);
    problems.extend(helper_problems.drain(..).map(|message| Problem { line: 1, message }));
    problems.sort();
    problems.dedup();
    problems
}

#[derive(Default)]
struct CatalogRows {
    ids: BTreeMap<String, String>,
    plurals: BTreeMap<String, String>,
    problems: Vec<String>,
}

fn parse_catalog(path: &Path) -> Result<CatalogRows, String> {
    let contents = fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut rows = CatalogRows::default();
    for (line_index, line) in contents.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        match fields.first().copied() {
            Some("@id") if fields.len() == 3 => {
                let id = fields[1];
                if id.is_empty() || fields[2].is_empty() {
                    rows.problems.push(format!("{}:{}: @id rows need a non-empty ID and value", path.display(), line_index + 1));
                } else if rows.ids.insert(id.to_string(), fields[2].to_string()).is_some() {
                    rows.problems.push(format!("{}:{}: duplicate catalog ID {id:?}", path.display(), line_index + 1));
                }
            }
            Some("@plural") if fields.len() == 3 => {
                let key = fields[1];
                if !key.contains('|') || fields[2].is_empty() {
                    rows.problems.push(format!("{}:{}: @plural rows need singular|plural sources and a value", path.display(), line_index + 1));
                } else if rows.plurals.insert(key.to_string(), fields[2].to_string()).is_some() {
                    rows.problems.push(format!("{}:{}: duplicate plural source {key:?}", path.display(), line_index + 1));
                }
            }
            Some(directive) if directive.starts_with('@') => {
                rows.problems.push(format!("{}:{}: unsupported or malformed catalog row", path.display(), line_index + 1));
            }
            _ => rows.problems.push(format!("{}:{}: expected an @id or @plural catalog row", path.display(), line_index + 1)),
        }
    }
    Ok(rows)
}

fn catalog_problems(english: &BTreeMap<String, String>, locales: &BTreeMap<String, BTreeMap<String, String>>) -> Vec<String> {
    let mut problems = Vec::new();
    for code in ["en-XA", "ar-XB"] {
        let Some(rows) = locales.get(code) else {
            problems.push(format!("missing required pseudo catalog {code}"));
            continue;
        };
        for id in english.keys() {
            if !rows.contains_key(id) {
                problems.push(format!("{code} is missing English UI ID {id}"));
            }
        }
        for id in rows.keys() {
            if !english.contains_key(id) {
                problems.push(format!("{code} has stale UI ID {id}"));
            }
        }
    }
    problems
}

fn load_catalogs(root: &Path) -> Result<Catalogs, Vec<String>> {
    let directory = root.join("i18n");
    let mut paths = Vec::new();
    let entries = fs::read_dir(&directory).map_err(|error| vec![format!("read {}: {error}", directory.display())])?;
    for entry in entries {
        let entry = entry.map_err(|error| vec![format!("read i18n catalog entry: {error}")])?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "tsv") {
            paths.push(path);
        }
    }
    paths.sort();
    let mut catalogs = Catalogs::default();
    let mut problems = Vec::new();
    for path in paths {
        let code = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default().to_string();
        let rows = match parse_catalog(&path) {
            Ok(rows) => rows,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        problems.extend(rows.problems);
        if code == "en" {
            catalogs.english = rows.ids;
            catalogs.english_plurals = rows.plurals.keys().cloned().collect();
        } else {
            catalogs.locales.insert(code.clone(), rows.ids);
            catalogs.locale_plurals.insert(code, rows.plurals.keys().cloned().collect());
        }
    }
    if catalogs.english.is_empty() {
        problems.push("i18n/en.tsv is missing or has no @id rows".into());
    }
    if problems.is_empty() { Ok(catalogs) } else { Err(problems) }
}

fn validate_catalogs(catalogs: &Catalogs) -> Vec<String> {
    let mut problems = catalog_problems(&catalogs.english, &catalogs.locales);
    for code in catalogs.locales.keys() {
        let locale = &catalogs.locales[code];
        for id in locale.keys() {
            if !catalogs.english.contains_key(id) {
                problems.push(format!("{code} has stale UI ID {id}"));
            }
        }
        if let Some(plurals) = catalogs.locale_plurals.get(code) {
            for source in plurals {
                if !catalogs.english_plurals.contains(source) {
                    problems.push(format!("{code} has stale plural source {source:?}"));
                }
            }
            if matches!(code.as_str(), "en-XA" | "ar-XB") {
                for source in catalogs.english_plurals.difference(plurals) {
                    problems.push(format!("{code} is missing English plural source {source:?}"));
                }
            }
        }
        if matches!(code.as_str(), "en-XA" | "ar-XB") {
            for (id, value) in locale {
                if id == "language.name" {
                    continue;
                }
                let marked = if code == "en-XA" {
                    value.starts_with("$$$") && value.ends_with("@@@")
                } else {
                    value.starts_with("[[RLI]]$$$") && value.ends_with("@@@[[PDI]]")
                };
                if !marked {
                    problems.push(format!("{code} UI ID {id} is missing pseudo markers"));
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

fn collect_rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for relative in ["crates/ui-egui/src", "apps/notecraft/src"] {
        let directory = root.join(relative);
        if directory.is_dir() {
            super::files_under(root, &directory, &mut files)?;
        }
    }
    files.retain(|path| path.extension().is_some_and(|extension| extension == "rs"));
    files.sort();
    Ok(files)
}

pub(crate) fn run(root: &Path) -> Result<(), String> {
    let catalogs = load_catalogs(root).map_err(|problems| problems.join("\n"))?;
    let mut problems = validate_catalogs(&catalogs);
    let english_ids: BTreeSet<String> = catalogs.english.keys().cloned().collect();
    let files = collect_rust_files(root)?;
    let mut sources = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let parsed = syn::parse_file(&source).map_err(|error| format!("parse {}: {error}", path.display()))?;
        let (found, found_problems) = source_helpers(&parsed, &source);
        for problem in found_problems {
            problems.push(format!("{}: {problem}", path.strip_prefix(root).unwrap_or(path).display()));
        }
        sources.push((path.clone(), source, parsed, found));
    }
    let mut findings = 0usize;
    for (path, source, parsed, helpers) in &sources {
        let relative = path.strip_prefix(root).unwrap_or(path).display().to_string();
        let source_problems = scan_parsed_source(source, parsed, &english_ids, helpers);
        for problem in source_problems {
            let report = format!("{relative}:{}: {}", problem.line, problem.message);
            if problem.message.starts_with("hard-coded UI string") {
                findings += 1;
                println!("i18n-audit: pending: {report}");
            } else {
                problems.push(report);
            }
        }
    }
    for (code, rows) in &catalogs.locales {
        if matches!(code.as_str(), "en-XA" | "ar-XB") {
            continue;
        }
        let missing = catalogs.english.keys().filter(|id| !rows.contains_key(*id)).count();
        let actual_plurals = catalogs.locale_plurals.get(code);
        let missing_plural = catalogs.english_plurals.iter().filter(|source| actual_plurals.is_none_or(|actual| !actual.contains(*source))).count();
        if missing > 0 || missing_plural > 0 {
            println!("i18n-audit: {code}: {missing} untranslated IDs, {missing_plural} untranslated plural forms (English fallback)");
        }
    }
    println!("i18n-audit: checked {} Rust files, {} English IDs, {} source findings", files.len(), catalogs.english.len(), findings);
    if problems.is_empty() {
        if findings == 0 {
            println!("i18n-audit: passed with no source findings");
        } else {
            println!("i18n-audit: passed structural checks; {findings} hard-coded UI strings remain for localization");
        }
        Ok(())
    } else {
        problems.sort();
        problems.dedup();
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_ui_literal_is_rejected() {
        let ids = BTreeSet::new();
        let problems = scan_source("sample.rs", "fn draw(ui: &mut Ui) { ui.label(\"Save\"); }", &ids);
        assert!(problems.iter().any(|problem| problem.message.contains("hard-coded UI string")));
    }

    #[test]
    fn catalog_id_lookup_is_accepted_but_unknown_ids_are_rejected() {
        let ids = BTreeSet::from(["ui.save".to_string()]);
        let accepted = scan_source("sample.rs", "fn draw(ui: &mut Ui) { ui.label(crate::tl_id!(\"ui.save\")); }", &ids);
        let rejected = scan_source("sample.rs", "fn draw(ui: &mut Ui) { ui.label(crate::tl_id!(\"ui.svae\")); }", &ids);
        assert!(accepted.is_empty());
        assert!(rejected.iter().any(|problem| problem.message.contains("unknown UI ID")));
    }

    #[test]
    fn annotated_helper_literal_is_rejected() {
        let source = "// i18n-ui-text: label,tip\nfn action(label: &str, tip: &str) {}\nfn draw() { action(\"Save\", \"Save the page\"); }";
        let problems = scan_source("sample.rs", source, &BTreeSet::new());
        assert_eq!(problems.iter().filter(|problem| problem.message.contains("hard-coded UI string")).count(), 2);
    }

    #[test]
    fn documented_exemption_allows_a_non_language_glyph() {
        let source = "fn draw(ui: &mut Ui) { ui.label(\"T\"); // i18n-exempt: single-letter title glyph\n}";
        assert!(scan_source("sample.rs", source, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn exemption_without_a_reason_is_rejected() {
        let source = "fn draw(ui: &mut Ui) { ui.label(\"T\"); // i18n-exempt:\n}";
        assert!(scan_source("sample.rs", source, &BTreeSet::new()).iter().any(|problem| problem.message.contains("requires a reason")));
    }

    #[test]
    fn pseudo_catalogs_must_match_the_english_id_set() {
        let english = BTreeMap::from([("ui.save".to_string(), "Save".to_string()), ("ui.open".to_string(), "Open".to_string())]);
        let locales = BTreeMap::from([("en-XA".to_string(), BTreeMap::from([("ui.save".to_string(), "$$$Save@@@".to_string())]))]);
        let problems = catalog_problems(&english, &locales);
        assert!(problems.iter().any(|problem| problem.contains("en-XA") && problem.contains("ui.open")));
    }
}
