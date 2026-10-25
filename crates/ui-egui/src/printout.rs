//! Document printout rendering for dropped files.
//!
//! The page editor owns insertion and persistence. This module owns the renderer boundary,
//! keeps temporary output private, and never reports a partial printout as complete.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderSettings, render as render_pdf_page};

const OFFICE_EXTENSIONS: &[&str] = &["doc", "docx", "ppt", "pptx", "xls", "xlsx", "odt", "odp", "ods", "rtf"];

pub(crate) struct Rendered {
    workspace: PathBuf,
    pages: Vec<PathBuf>,
}

impl Rendered {
    pub(crate) fn pages(&self) -> &[PathBuf] {
        &self.pages
    }
}

impl Drop for Rendered {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}

pub(crate) fn supports(source: &Path) -> bool {
    let Some(extension) = source.extension().and_then(OsStr::to_str) else { return false };
    let extension = extension.to_ascii_lowercase();
    extension == "pdf" || OFFICE_EXTENSIONS.contains(&extension.as_str())
}

pub(crate) fn render(source: &Path) -> Result<Rendered, String> {
    if !supports(source) {
        return Err("Printout is supported for PDF and Office documents only".into());
    }
    let workspace = temporary_workspace()?;
    fs::create_dir_all(&workspace).map_err(|e| format!("cannot create printout workspace: {e}"))?;
    let result = render_in_workspace(source, &workspace);
    match result {
        Ok(pages) => Ok(Rendered { workspace, pages }),
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            Err(error)
        }
    }
}

fn render_in_workspace(source: &Path, workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let pdf = if source.extension().and_then(OsStr::to_str).is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        source.to_path_buf()
    } else {
        office_to_pdf(source, workspace)?
    };
    let pages = render_pdf(&pdf, workspace)?;
    if pages.is_empty() {
        return Err("the renderer produced no printout pages".into());
    }
    Ok(pages)
}

fn render_pdf(pdf_path: &Path, workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let data = fs::read(pdf_path).map_err(|e| format!("cannot read PDF for rendering: {e}"))?;
    let pdf = Pdf::new(Arc::new(data)).map_err(|e| format!("cannot parse PDF for rendering: {e:?}"))?;
    let pages_dir = workspace.join("pages");
    fs::create_dir_all(&pages_dir).map_err(|e| format!("cannot create printout pages folder: {e}"))?;
    let interpreter = InterpreterSettings::default();
    let settings = RenderSettings { x_scale: 2.0, y_scale: 2.0, bg_color: WHITE, ..Default::default() };
    let mut pages = Vec::with_capacity(pdf.pages().len());
    for (index, page) in pdf.pages().iter().enumerate() {
        let pixmap = render_pdf_page(page, &interpreter, &settings);
        let path = pages_dir.join(format!("page-{:03}.png", index + 1));
        let png = pixmap.into_png().map_err(|e| format!("cannot encode PDF page {} as PNG: {e}", index + 1))?;
        fs::write(&path, png).map_err(|e| format!("cannot write rendered PDF page {}: {e}", index + 1))?;
        pages.push(path);
    }
    Ok(pages)
}

fn office_to_pdf(source: &Path, workspace: &Path) -> Result<PathBuf, String> {
    let mut last_error = String::new();
    for executable in office_renderers() {
        match Command::new(&executable).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(workspace).arg(source).output() {
            Ok(output) if output.status.success() => {
                let stem: OsString = source.file_stem().map(OsStr::to_os_string).unwrap_or_else(|| OsString::from("printout"));
                let pdf = workspace.join(stem).with_extension("pdf");
                if pdf.is_file() {
                    return Ok(pdf);
                }
                last_error = format!("{} reported success but produced no PDF", executable.to_string_lossy());
            }
            Ok(output) => last_error = format!("{}: {}", executable.to_string_lossy(), String::from_utf8_lossy(&output.stderr).trim()),
            Err(error) => last_error = format!("{}: {error}", executable.to_string_lossy()),
        }
    }
    Err(format!("cannot render Office document. Install LibreOffice (soffice) and ensure it is available: {last_error}"))
}

fn office_renderers() -> Vec<OsString> {
    let mut renderers = vec![OsString::from("soffice"), OsString::from("libreoffice")];
    #[cfg(windows)]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            renderers.push(Path::new(&program_files).join("LibreOffice/program/soffice.exe").into_os_string());
        }
        if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            renderers.push(Path::new(&program_files_x86).join("LibreOffice/program/soffice.exe").into_os_string());
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            renderers.push(Path::new(&local_app_data).join("Programs/LibreOffice/program/soffice.exe").into_os_string());
        }
    }
    renderers
}

fn temporary_workspace() -> Result<PathBuf, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("clock error: {e}"))?.as_nanos();
    Ok(std::env::temp_dir().join(format!("notecraft-printout-{}-{stamp}", std::process::id())))
}

#[cfg(test)]
mod tests {
    use super::{render_pdf, supports, temporary_workspace};
    use std::fs;
    use std::io::Write as IoWrite;
    use std::path::Path;

    #[test]
    fn supports_pdf_and_office_documents() {
        for path in ["brief.pdf", "brief.docx", "slides.pptx", "sheet.xlsx", "text.rtf"] {
            assert!(supports(Path::new(path)), "{path}");
        }
        assert!(!supports(Path::new("picture.png")));
        assert!(!supports(Path::new("notes.txt")));
    }

    #[test]
    fn renders_pdf_pages_to_png() {
        let workspace = temporary_workspace().expect("temporary workspace");
        fs::create_dir_all(&workspace).expect("create temporary workspace");
        let pdf_path = workspace.join("sample.pdf");
        fs::write(&pdf_path, minimal_pdf()).expect("write sample PDF");

        let pages = render_pdf(&pdf_path, &workspace).expect("render sample PDF");

        assert_eq!(pages.len(), 1);
        assert!(fs::read(&pages[0]).expect("read PNG").starts_with(b"\x89PNG\r\n\x1a\n"));
        fs::remove_dir_all(workspace).expect("remove temporary workspace");
    }

    fn minimal_pdf() -> Vec<u8> {
        let content = "BT /F1 12 Tf 72 720 Td (Hello) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0usize];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            write!(pdf, "{} 0 obj\n{}\nendobj\n", index + 1, object).expect("write PDF object");
        }
        let xref_offset = pdf.len();
        write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", offsets.len()).expect("write PDF xref");
        for offset in offsets.iter().skip(1) {
            writeln!(pdf, "{offset:010} 00000 n ").expect("write PDF xref entry");
        }
        write!(pdf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n", offsets.len()).expect("write PDF trailer");
        pdf
    }
}
