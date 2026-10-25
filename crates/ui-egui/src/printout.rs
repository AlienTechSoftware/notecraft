//! Document printout rendering for dropped files.
//!
//! The page editor owns insertion and persistence. This module owns the renderer boundary:
//! it uses installed command-line renderers, keeps their temporary output private, and never
//! reports a partial printout as complete.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let pages_dir = workspace.join("pages");
    fs::create_dir_all(&pages_dir).map_err(|e| format!("cannot create printout pages folder: {e}"))?;
    let prefix = pages_dir.join("page");
    let output = Command::new(pdf_rasterizer())
        .args(["-png", "-r", "144"])
        .arg(&pdf)
        .arg(&prefix)
        .output()
        .map_err(|e| format!("cannot start PDF rasterizer: {e}. Install Poppler and ensure pdftoppm is available"))?;
    if !output.status.success() {
        return Err(format!("PDF rasterizer could not render the printout: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    let mut pages: Vec<PathBuf> = fs::read_dir(&pages_dir)
        .map_err(|e| format!("cannot read rendered printout pages: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str).is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .collect();
    pages.sort_by_key(|path| page_number(path).unwrap_or(u32::MAX));
    if pages.is_empty() {
        return Err("the renderer produced no printout pages".into());
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

fn pdf_rasterizer() -> OsString {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("renderers/poppler/Library/bin/pdftoppm.exe"));
    if let Some(path) = std::env::var_os("NOTECRAFT_PDFTOPPM").map(PathBuf::from).or(bundled)
        && path.is_file()
    {
        return path.into_os_string();
    }
    OsString::from("pdftoppm")
}

fn page_number(path: &Path) -> Option<u32> {
    path.file_stem()?.to_str()?.rsplit('-').next()?.parse().ok()
}

fn temporary_workspace() -> Result<PathBuf, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("clock error: {e}"))?.as_nanos();
    Ok(std::env::temp_dir().join(format!("notecraft-printout-{}-{stamp}", std::process::id())))
}

#[cfg(test)]
mod tests {
    use super::supports;
    use std::path::Path;

    #[test]
    fn supports_pdf_and_office_documents() {
        for path in ["brief.pdf", "brief.docx", "slides.pptx", "sheet.xlsx", "text.rtf"] {
            assert!(supports(Path::new(path)), "{path}");
        }
        assert!(!supports(Path::new("picture.png")));
        assert!(!supports(Path::new("notes.txt")));
    }
}
