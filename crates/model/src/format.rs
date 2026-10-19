//! The two page formats. HTML is the owner's preferred default (2026-10-17).

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageFormat {
    /// `index.html`
    Html,
    /// CommonMark plus GitHub Flavored Markdown, stored as `index.md`.
    Cmgfm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

impl PageFormat {
    /// The built-in default when no setting says otherwise.
    pub const DEFAULT: PageFormat = PageFormat::Html;
    pub const ALL: [PageFormat; 2] = [PageFormat::Html, PageFormat::Cmgfm];

    /// The name stored in `_node.toml` and in settings.
    pub fn id(self) -> &'static str {
        match self {
            PageFormat::Html => "html",
            PageFormat::Cmgfm => "cmgfm",
        }
    }

    /// What the page's own file is called in its folder.
    pub fn page_file(self) -> &'static str {
        match self {
            PageFormat::Html => "index.html",
            PageFormat::Cmgfm => "index.md",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PageFormat::Html => "HTML",
            PageFormat::Cmgfm => "CommonMark + GFM",
        }
    }

    /// Short badge text for lists.
    pub fn short(self) -> &'static str {
        match self {
            PageFormat::Html => "html",
            PageFormat::Cmgfm => "md",
        }
    }

    /// The content file of a page in this format.
    pub fn file_name(self) -> &'static str {
        match self {
            PageFormat::Html => "index.html",
            PageFormat::Cmgfm => "index.md",
        }
    }

    /// Accepts `html`, `cmgfm` and the aliases `markdown`, `md`, `gfm`, `commonmark`.
    pub fn parse(text: &str) -> Result<PageFormat, FormatError> {
        match text.trim().to_lowercase().as_str() {
            "html" => Ok(PageFormat::Html),
            "cmgfm" | "markdown" | "md" | "gfm" | "commonmark" => Ok(PageFormat::Cmgfm),
            _ => Err(FormatError(format!("unknown format '{text}'; allowed: html, cmgfm (aliases: markdown, md, gfm, commonmark)"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_html() {
        assert_eq!(PageFormat::DEFAULT, PageFormat::Html);
    }

    #[test]
    fn aliases_parse() {
        assert_eq!(PageFormat::parse("Markdown"), Ok(PageFormat::Cmgfm));
        assert_eq!(PageFormat::parse(" MD "), Ok(PageFormat::Cmgfm));
        assert_eq!(PageFormat::parse("html"), Ok(PageFormat::Html));
        assert!(PageFormat::parse("docx").is_err());
    }

    #[test]
    fn file_names() {
        assert_eq!(PageFormat::Html.file_name(), "index.html");
        assert_eq!(PageFormat::Cmgfm.file_name(), "index.md");
    }
}
