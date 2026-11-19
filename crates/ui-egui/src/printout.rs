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

/// Renders `source` (a PDF, or an Office document by way of a PDF) to one picture a page, in a
/// workspace of its own that goes with the result, telling `progress` how many pages of how many
/// are done as they finish.
pub(crate) fn render_with(source: &Path, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Rendered, String> {
    if !supports(source) {
        return Err("Printout is supported for PDF and Office documents only".into());
    }
    let workspace = temporary_workspace()?;
    fs::create_dir_all(&workspace).map_err(|e| format!("cannot create printout workspace: {e}"))?;
    let result = render_in_workspace(source, &workspace, progress);
    match result {
        Ok(pages) => Ok(Rendered { workspace, pages }),
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            Err(error)
        }
    }
}

fn render_in_workspace(source: &Path, workspace: &Path, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Vec<PathBuf>, String> {
    let pdf = if source.extension().and_then(OsStr::to_str).is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        source.to_path_buf()
    } else {
        office_to_pdf(source, workspace)?
    };
    let pages = render_pdf_with(&pdf, workspace, progress)?;
    if pages.is_empty() {
        return Err("the renderer produced no printout pages".into());
    }
    Ok(pages)
}

#[cfg(test)]
fn render_pdf(pdf_path: &Path, workspace: &Path) -> Result<Vec<PathBuf>, String> {
    render_pdf_with(pdf_path, workspace, &|_, _| {})
}

/// Renders every page of a PDF to a PNG in `workspace`, in the document's order. The pages are
/// rendered side by side, one thread to a core, each with its own reading of the document: one
/// after another, a fifteen-page printout took about two seconds.
fn render_pdf_with(pdf_path: &Path, workspace: &Path, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Vec<PathBuf>, String> {
    let data = Arc::new(fs::read(pdf_path).map_err(|e| format!("cannot read PDF for rendering: {e}"))?);
    let count = Pdf::new(data.clone()).map_err(|e| format!("cannot parse PDF for rendering: {e:?}"))?.pages().len();
    let pages_dir = workspace.join("pages");
    fs::create_dir_all(&pages_dir).map_err(|e| format!("cannot create printout pages folder: {e}"))?;
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).clamp(1, 8).min(count.max(1));
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<Result<Vec<(usize, PathBuf)>, String>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|worker| {
                let (data, pages_dir, done) = (data.clone(), &pages_dir, &done);
                scope.spawn(move || -> Result<Vec<(usize, PathBuf)>, String> {
                    let pdf = Pdf::new(data).map_err(|e| format!("cannot parse PDF for rendering: {e:?}"))?;
                    let interpreter = InterpreterSettings::default();
                    let settings = RenderSettings { x_scale: 2.0, y_scale: 2.0, bg_color: WHITE, ..Default::default() };
                    let mut mine = Vec::new();
                    for (index, page) in pdf.pages().iter().enumerate().skip(worker).step_by(threads) {
                        let pixmap = render_pdf_page(page, &interpreter, &settings);
                        let path = pages_dir.join(format!("page-{:03}.png", index + 1));
                        let png = pixmap.into_png().map_err(|e| format!("cannot encode PDF page {} as PNG: {e}", index + 1))?;
                        fs::write(&path, png).map_err(|e| format!("cannot write rendered PDF page {}: {e}", index + 1))?;
                        mine.push((index, path));
                        progress(done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1, count);
                    }
                    Ok(mine)
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap_or_else(|_| Err("a page could not be rendered".to_string()))).collect()
    });
    let mut pages: Vec<(usize, PathBuf)> = Vec::with_capacity(count);
    for result in results {
        pages.extend(result?);
    }
    pages.sort_by_key(|(index, _)| *index);
    Ok(pages.into_iter().map(|(_, path)| path).collect())
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
    #[cfg(windows)]
    {
        let mut renderers = vec![OsString::from("soffice"), OsString::from("libreoffice")];
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            renderers.push(Path::new(&program_files).join("LibreOffice/program/soffice.exe").into_os_string());
        }
        if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            renderers.push(Path::new(&program_files_x86).join("LibreOffice/program/soffice.exe").into_os_string());
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            renderers.push(Path::new(&local_app_data).join("Programs/LibreOffice/program/soffice.exe").into_os_string());
        }
        renderers
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from("soffice"), OsString::from("libreoffice")]
    }
}

/// A printout being made off the window's thread: rendered, its document attached to the page,
/// and its pages copied into the page's folder. On a synced folder the copying alone took over a
/// second for fifteen pages, and the whole of it froze the page until it was done.
pub(crate) struct Job {
    /// The page it is for, and the name of the document.
    pub dir: PathBuf,
    pub name: String,
    /// How many pages are rendered, of how many, as last heard.
    pub pages: (usize, usize),
    news: std::sync::mpsc::Receiver<News>,
}

/// What a printout job is done with: the document's link in the page's folder and the links of
/// its pages, in order.
pub(crate) struct Made {
    pub link: String,
    pub images: Vec<String>,
}

enum News {
    Pages(usize, usize),
    Done(Result<Made, String>),
}

impl Job {
    /// Starts making a printout of `file` for the page in `dir`.
    pub(crate) fn start(dir: &Path, file: &Path) -> Job {
        let (tell, news) = std::sync::mpsc::channel();
        let (page, source) = (dir.to_path_buf(), file.to_path_buf());
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let job_name = name.clone();
        let spawned = std::thread::Builder::new().name("notecraft-printout".into()).spawn({
            let tell = tell.clone();
            move || {
                let progress = std::sync::Mutex::new(tell.clone());
                let made = make(&page, &source, &job_name, &|done, total| {
                    if let Ok(tell) = progress.lock() {
                        let _ = tell.send(News::Pages(done, total));
                    }
                });
                let _ = tell.send(News::Done(made));
            }
        });
        if let Err(e) = spawned {
            let _ = tell.send(News::Done(Err(format!("cannot start making the printout: {e}"))));
        }
        Job { dir: dir.to_path_buf(), name, pages: (0, 0), news }
    }

    /// What has happened since this was last asked: `Some` once the job is done.
    pub(crate) fn poll(&mut self) -> Option<Result<Made, String>> {
        loop {
            match self.news.try_recv() {
                Ok(News::Pages(done, total)) => self.pages = (done, total),
                Ok(News::Done(made)) => return Some(made),
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Some(Err("the printout stopped before it was done".into())),
            }
        }
    }
}

/// Renders `source`, attaches it to the page in `dir`, and copies its pages into the page's
/// folder, taking them back out again if any could not be stored.
fn make(dir: &Path, source: &Path, name: &str, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Made, String> {
    let rendered = render_with(source, progress)?;
    let link = notecraft_store::assets::attach_file(dir, source)
        .map_err(|_| crate::i18n::format_message_id("ui.could_not_attach_name_before_rendering_its_pages", &[("name", name)]))?;
    let base = link
        .strip_prefix("assets/")
        .and_then(|link| Path::new(link).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| crate::tl_id!("ui.the_attachment_returned_an_invalid_page_link").to_string())?;
    let assets = dir.join("assets");
    let mut stored: Vec<PathBuf> = Vec::with_capacity(rendered.pages().len());
    for (index, page) in rendered.pages().iter().enumerate() {
        let target = assets.join(format!("{base}.printout-{:03}.png", index + 1));
        if let Err(error) = fs::copy(page, &target) {
            for path in &stored {
                let _ = fs::remove_file(path);
            }
            return Err(crate::i18n::format_message_id(
                "ui.could_not_store_printout_page_page_error",
                &[("page", &(index + 1).to_string()), ("error", &error.to_string())],
            ));
        }
        stored.push(target);
    }
    let images = (1..=stored.len()).map(|n| format!("assets/{base}.printout-{n:03}.png")).collect();
    Ok(Made { link, images })
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

    /// The pages of a printout are rendered side by side, one thread to a core, and still come
    /// out in the document's order.
    #[test]
    fn many_pages_render_side_by_side_in_order() {
        let workspace = temporary_workspace().expect("temporary workspace");
        fs::create_dir_all(&workspace).expect("create temporary workspace");
        let pdf_path = workspace.join("colours.pdf");
        let colours = [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (1.0, 1.0, 0.0), (0.0, 1.0, 1.0), (1.0, 0.0, 1.0), (0.0, 0.0, 0.0)];
        fs::write(&pdf_path, pdf_of_colours(&colours)).expect("write sample PDF");
        let seen = std::sync::atomic::AtomicUsize::new(0);
        let pages = super::render_pdf_with(&pdf_path, &workspace, &|done, total| {
            assert_eq!(total, colours.len());
            seen.fetch_max(done, std::sync::atomic::Ordering::SeqCst);
        })
        .expect("render");
        assert_eq!(pages.len(), colours.len());
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), colours.len(), "progress reached the last page");
        for (page, (r, g, b)) in pages.iter().zip(colours) {
            let img = image::open(page).expect("a PNG").to_rgb8();
            let px = img.get_pixel(img.width() / 2, img.height() / 2).0;
            let want = [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8];
            assert_eq!(px, want, "{page:?} is the page of that colour");
        }
        fs::remove_dir_all(workspace).expect("remove temporary workspace");
    }

    /// A PDF of one page per colour, each filled with it.
    fn pdf_of_colours(colours: &[(f32, f32, f32)]) -> Vec<u8> {
        let n = colours.len();
        let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned()];
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
        objects.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")));
        for (i, (r, g, b)) in colours.iter().enumerate() {
            let content = format!("{r} {g} {b} rg 0 0 200 200 re f");
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R >>", 4 + 2 * i));
            objects.push(format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content));
        }
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
