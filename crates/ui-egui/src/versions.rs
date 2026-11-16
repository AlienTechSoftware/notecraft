//! Looking at a saved page version before restoring it (issue #278).
//!
//! History > Page Versions listed saved copies with only a Restore button, so the one way to see
//! what a copy held was to put it back over the page. A copy can now be opened read-only, clearly
//! marked as an older version, and compared with the page line by line; Restore stays a separate,
//! explicit choice, and closing the preview leaves the page exactly as it was.

use notecraft_formats::Block;
use notecraft_model::PageFormat;

/// A saved version opened for reading.
#[derive(Clone, Debug)]
pub struct Preview {
    pub id: String,
    /// When it was saved, as the list shows it.
    pub when: String,
    pub blocks: Vec<Block>,
    /// Its lines against the page's, for the comparison.
    pub changes: Vec<Change>,
    /// Showing the comparison rather than the version itself.
    pub comparing: bool,
}

/// One line of the comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// In both.
    Same(String),
    /// Only in the saved version: restoring brings it back.
    OnlyThen(String),
    /// Only in the page as it is now: restoring takes it away.
    OnlyNow(String),
}

/// The most lines either side is compared over, so a huge page cannot stall the window.
const MAX_LINES: usize = 4000;

/// A page body as lines of readable text: HTML is read as the Markdown it stands for, so tags
/// do not crowd the comparison. Blank lines carry nothing and are left out.
pub fn lines_of(format: PageFormat, body: &str) -> Vec<String> {
    let text = match format {
        PageFormat::Html => notecraft_formats::html::to_markdown(body),
        PageFormat::Cmgfm => body.to_string(),
    };
    text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).map(str::to_string).take(MAX_LINES).collect()
}

/// The lines of `then` and `now` in order, each marked as in both or in one only: the
/// longest common run of lines is kept, as `diff` does.
pub fn compare(then: &[String], now: &[String]) -> Vec<Change> {
    let (n, m) = (then.len(), now.len());
    // lcs[i][j]: the longest common run of then[i..] and now[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if then[i] == now[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(n.max(m));
    while i < n && j < m {
        if then[i] == now[j] {
            out.push(Change::Same(then[i].clone()));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(Change::OnlyThen(then[i].clone()));
            i += 1;
        } else {
            out.push(Change::OnlyNow(now[j].clone()));
            j += 1;
        }
    }
    out.extend(then[i..].iter().cloned().map(Change::OnlyThen));
    out.extend(now[j..].iter().cloned().map(Change::OnlyNow));
    out
}

/// When a version was saved, on the user's clock ("Saturday, 17 October 2026, 18:29"), or as
/// written when it cannot be read.
pub fn when(created: &str) -> String {
    notecraft_model::time::long_from_iso(created).unwrap_or_else(|| created.to_string())
}

/// Opens version `id` of the page at `page` for reading, against the page's body `now`.
pub fn open(page: &std::path::Path, id: &str, created: &str, now: &str) -> Result<Preview, String> {
    let (format, body) = notecraft_store::page::version_body(page, id).map_err(|e| e.to_string())?;
    let changes = compare(&lines_of(format, &body), &lines_of(format, now));
    Ok(Preview { id: id.to_string(), when: when(created), blocks: crate::preview::blocks_for(format, &body), changes, comparing: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn the_comparison_keeps_what_both_have_and_marks_the_rest() {
        let then = lines("a\nb\nc\nd");
        let now = lines("a\nc\nd\ne");
        assert_eq!(
            compare(&then, &now),
            vec![
                Change::Same("a".into()),
                Change::OnlyThen("b".into()),
                Change::Same("c".into()),
                Change::Same("d".into()),
                Change::OnlyNow("e".into())
            ]
        );
        assert_eq!(compare(&[], &lines("x")), vec![Change::OnlyNow("x".into())]);
        assert_eq!(compare(&lines("x"), &[]), vec![Change::OnlyThen("x".into())]);
        assert!(compare(&then, &then).iter().all(|c| matches!(c, Change::Same(_))));
    }

    #[test]
    fn html_is_compared_as_the_words_it_holds() {
        let got = lines_of(PageFormat::Html, "<p>alpha</p>\n\n<p><strong>bravo</strong></p>");
        assert_eq!(got, ["alpha", "**bravo**"]);
        assert_eq!(lines_of(PageFormat::Cmgfm, "one\n\n\ntwo  \n"), ["one", "two"]);
    }

    #[test]
    fn a_saved_time_is_shown_on_the_clock_or_as_written() {
        assert_ne!(when("2026-11-14T10:00:00Z"), "2026-11-14T10:00:00Z");
        assert_eq!(when("whenever"), "whenever");
    }
}
