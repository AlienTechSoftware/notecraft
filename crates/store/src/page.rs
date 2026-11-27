//! Reading and writing the content of a page.

use crate::error::{Result, StoreError};
use crate::tree::{page_file, raw_title, read_node};
use notecraft_formats::{boxes, frontmatter, html};
use notecraft_model::{Container, Dir, PageFormat, container, id::new_id, justify, kv, tablecols, time::iso_now};
use std::fs;
use std::path::{Path, PathBuf};

/// Pages larger than this are refused (a guard against hostile or accidental input).
pub const MAX_PAGE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub format: PageFormat,
    /// The title shown for this page: a real title, or `tree::UNTITLED_PAGE` when it has
    /// none. For the window's title area, which is drawn blank rather than showing that
    /// fallback as if it were content, see `has_title`.
    pub title: String,
    /// True when the page carries a title of its own. `false` means `title` is only the
    /// fallback shown in the tree and the page list; the window draws its title area blank
    /// in that case rather than putting `tree::UNTITLED_PAGE` into an editable box, which
    /// would read as content someone had typed.
    pub has_title: bool,
    /// The editable part: the inside of the HTML body, or the Markdown after the front matter.
    pub body: String,
    /// Which way the whole page reads, if the writer said. `None` means each line decides
    /// for itself, from its own first letter.
    pub dir: Option<Dir>,
    /// Whether the page is locked read only, as its own head says (issue #441).
    pub read_only: bool,
}

/// A saved copy of a page file, newest first. The page's own format and assets remain
/// ordinary files; versions are kept in its hidden `.notecraft/versions` folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub id: String,
    pub created: String,
    pub bytes: u64,
    /// How many of the page's own files (its ink and attachments) the version keeps beside its
    /// words (issue #553). `None` for a version taken before versions kept them: its words
    /// alone can be brought back.
    pub files: Option<usize>,
}

const MAX_VERSIONS: usize = 50;
/// What a page's versions may take up, their page files and the copies of its ink and
/// attachments together; the newest version is kept whatever its size.
const MAX_VERSION_BYTES: u64 = 128 * 1024 * 1024;
const VERSION_INTERVAL_NS: u128 = 5 * 60 * 1_000_000_000;

pub fn load_page(dir: &Path) -> Result<Page> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let meta = fs::metadata(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if meta.len() > MAX_PAGE_BYTES as u64 {
        return Err(StoreError::invalid("this page is too large to open"));
    }
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = body_of(format, &text);
    let has_title = raw_title(format, &text).is_some();
    let title = read_node(dir).map(|i| i.title).unwrap_or_default();
    let read_only = read_only_in(format, &text);
    Ok(Page { format, title, has_title, body, dir: read_direction(&text, format), read_only })
}

/// Saved versions of the page's main file, newest first.
pub fn versions(dir: &Path) -> Result<Vec<Version>> {
    let (_, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let versions = version_dir(dir);
    let entries = match fs::read_dir(&versions) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::io("read", &versions, e)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::io("read", &versions, e))?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let info = dir.join("version.toml");
        let text = match fs::read_to_string(&info) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(StoreError::io("read", &info, e)),
        };
        let values = kv::parse(&text);
        let id = values.get("id").cloned().unwrap_or_default();
        if !valid_version_id(&id) || dir.file_name().and_then(|n| n.to_str()) != Some(id.as_str()) {
            continue;
        }
        let snapshot = dir.join(file.file_name().unwrap_or_default());
        let bytes = match fs::metadata(&snapshot) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(StoreError::io("read", &snapshot, e)),
        };
        // The files a version can bring back: those it kept a copy of. A list that cannot be read
        // whole counts as none, and restoring that version says why.
        let files = read_manifest(&dir).ok().flatten().map(|kept| kept.iter().filter(|k| k.blob.is_some()).count());
        found.push(Version { id, created: values.get("created").cloned().unwrap_or_default(), bytes, files });
    }
    found.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(found)
}

/// The editable body of a saved version, in the page's own format, to show it without
/// restoring it (issue #278). The page file is not touched.
pub fn version_body(dir: &Path, id: &str) -> Result<(PageFormat, String)> {
    if !valid_version_id(id) {
        return Err(StoreError::invalid("that page version id is not valid"));
    }
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let snapshot = version_dir(dir).join(id).join(file.file_name().unwrap_or_default());
    let meta = fs::metadata(&snapshot).map_err(|e| StoreError::io("read", &snapshot, e))?;
    if meta.len() > MAX_PAGE_BYTES as u64 {
        return Err(StoreError::invalid("this page version is too large to open"));
    }
    let text = fs::read_to_string(&snapshot).map_err(|e| StoreError::io("read", &snapshot, e))?;
    Ok((format, body_of(format, &text)))
}

/// Restores an earlier body without rolling back the page’s current metadata.
/// The current file is first preserved as another version.
pub fn restore_version(dir: &Path, id: &str) -> Result<()> {
    if !valid_version_id(id) {
        return Err(StoreError::invalid("that page version id is not valid"));
    }
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, body) = version_body(dir, id)?;
    let (_, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let current = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    // A locked page keeps its words, so an older version does not replace them (issue #441).
    if read_only_in(format, &current) {
        return Err(StoreError::invalid("this page is locked read only"));
    }
    let out = update_body(format, &current, &body, &iso_now(), true);
    if out.len() > MAX_PAGE_BYTES {
        return Err(StoreError::invalid("this page is too large to save"));
    }
    // The page's own files as the version kept them, every one read and checked before any is
    // written, so a damaged copy changes nothing (issue #553).
    let manifest = read_manifest(&version_dir(dir).join(id))?;
    if manifest.as_ref().is_some_and(|kept| kept.iter().any(|k| k.blob.is_some())) && !copies_are_safe(dir) {
        return Err(StoreError::invalid("the folder the page version's files are kept in is a link; nothing was restored"));
    }
    let mut put_back: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for kept in manifest.unwrap_or_default() {
        let Some(blob) = kept.blob else { continue };
        let target = companion_path(dir, &kept.path).ok_or_else(|| StoreError::invalid("a page version names a file outside its page"))?;
        let source = blobs_dir(dir).join(&blob);
        let bytes = fs::read(&source).map_err(|e| StoreError::io("read", &source, e))?;
        if bytes.len() as u64 != kept.bytes || !blob.starts_with(&content_name(&bytes)) {
            return Err(StoreError::invalid(format!("the copy of {} kept with that version is damaged; nothing was restored", kept.path)));
        }
        if fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
            put_back.push((target, bytes));
        }
    }
    // The page as it is now is kept as a version first, so the restore can itself be undone.
    snapshot_current(dir, &file, true)?;
    // Each file written beside its place first, under a name no other file has, so nothing is
    // overwritten until everything is ready.
    let mut staged: Vec<(PathBuf, PathBuf, Option<Vec<u8>>)> = Vec::new();
    let cleanup = |staged: &[(PathBuf, PathBuf, Option<Vec<u8>>)]| {
        for (_, tmp, _) in staged {
            let _ = fs::remove_file(tmp);
        }
    };
    for (target, bytes) in &put_back {
        let staging = target.parent().map(Path::to_path_buf).unwrap_or_else(|| dir.to_path_buf());
        if let Err(e) = fs::create_dir_all(&staging) {
            cleanup(&staged);
            return Err(StoreError::io("create", &staging, e));
        }
        let tmp = staging.join(format!(".restore-{}.tmp", new_id()));
        let written = fs::OpenOptions::new().write(true).create_new(true).open(&tmp).and_then(|mut f| std::io::Write::write_all(&mut f, bytes));
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            cleanup(&staged);
            return Err(StoreError::io("write", &tmp, e));
        }
        staged.push((target.clone(), tmp, fs::read(target).ok()));
    }
    if let Err(e) = write_atomic_file(&file, &out) {
        cleanup(&staged);
        return Err(e);
    }
    // Only the files the version kept: a file added since is left as it is. Should one not go
    // in, the page's words and the files already put back return to how they were.
    for (n, (target, tmp, _)) in staged.iter().enumerate() {
        if let Err(e) = fs::rename(tmp, target) {
            for (done, _, before) in staged[..n].iter().rev() {
                match before {
                    Some(bytes) => {
                        let _ = fs::write(done, bytes);
                    }
                    None => {
                        let _ = fs::remove_file(done);
                    }
                }
            }
            cleanup(&staged[n..]);
            let _ = write_atomic_file(&file, &current);
            return Err(StoreError::io("replace", target, e));
        }
    }
    Ok(())
}

/// The largest single file a version keeps a copy of; a larger one is named in the version as
/// not kept, and restoring leaves it as it is.
const MAX_COMPANION_BYTES: u64 = 32 * 1024 * 1024;

/// One of the page's own files as a version knows it: where it is in the page's folder, how
/// long it was, and the copy kept of it, if one was.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Kept {
    path: String,
    bytes: u64,
    blob: Option<String>,
}

/// The page's own files a version keeps beside its words (issue #553): its ink, and the files
/// in its `assets` folder, as paths in the page's folder with `/` between, in name order.
fn companions(dir: &Path) -> Vec<String> {
    let mut found = ink_files(dir);
    let assets = dir.join("assets");
    // A folder that is a link to somewhere else is not the page's own.
    if fs::symlink_metadata(&assets).is_ok_and(|m| m.file_type().is_symlink()) {
        return found;
    }
    if let Ok(entries) = fs::read_dir(&assets) {
        let mut assets: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|k| k.is_file()))
            // Not a file a restore is staging beside its place.
            .filter(|e| !e.file_name().to_string_lossy().starts_with(".restore-"))
            .map(|e| format!("assets/{}", e.file_name().to_string_lossy()))
            .collect();
        assets.sort();
        found.extend(assets);
    }
    found
}

/// A page's own file from its path in a version, only inside the page's folder: no part of the
/// way to it may be a link, which could lead outside the page.
fn companion_path(dir: &Path, path: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':']) {
            return None;
        }
        out.push(part);
        if fs::symlink_metadata(&out).is_ok_and(|m| m.file_type().is_symlink()) {
            return None;
        }
    }
    Some(out)
}

fn blobs_dir(dir: &Path) -> PathBuf {
    version_dir(dir).join("blobs")
}

/// Whether the folders the copies are kept in are the page's own: none of `.notecraft`, its
/// `versions` folder and the `blobs` folder in it is a link, which could lead outside the page.
/// When one is, no copy is kept, read or taken away.
fn copies_are_safe(dir: &Path) -> bool {
    let linked = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink());
    let hidden = dir.join(".notecraft");
    !linked(&hidden) && !linked(&version_dir(dir)) && !linked(&blobs_dir(dir))
}

/// A name for some bytes from what they are: two 64-bit FNV-1a sums with different starting
/// points, in hex. A copy is named by it, so the same file kept by many versions is kept once;
/// two different files that came to the same name are told apart by their bytes when kept.
fn content_name(bytes: &[u8]) -> String {
    let sum = |start: u64| bytes.iter().fold(start, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{:016x}{:016x}", sum(0xcbf2_9ce4_8422_2325), sum(0x84222325_cbf29ce4))
}

/// Keeps a copy of some bytes among the versions' copies, once, and gives its name.
fn keep_blob(dir: &Path, bytes: &[u8]) -> Result<String> {
    let blobs = blobs_dir(dir);
    fs::create_dir_all(&blobs).map_err(|e| StoreError::io("create", &blobs, e))?;
    let base = content_name(bytes);
    for n in 0..16 {
        let name = if n == 0 { base.clone() } else { format!("{base}-{n}") };
        let path = blobs.join(&name);
        match fs::read(&path) {
            Ok(there) if there == bytes => return Ok(name),
            Ok(_) => continue,
            Err(_) => {
                let tmp = blobs.join(format!("{name}.tmp"));
                fs::write(&tmp, bytes).map_err(|e| StoreError::io("write", &tmp, e))?;
                fs::rename(&tmp, &path).map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    StoreError::io("replace", &path, e)
                })?;
                return Ok(name);
            }
        }
    }
    Err(StoreError::invalid("too many different files share one name among the page's versions"))
}

/// The page's own files as they are now, with a copy of each kept, for a new version.
fn keep_companions(dir: &Path) -> Result<Vec<Kept>> {
    let mut kept = Vec::new();
    if !copies_are_safe(dir) {
        log::warn!("page versions keep the words alone: the folder for copies is a link: {dir:?}");
        return Ok(kept);
    }
    for path in companions(dir) {
        let Some(file) = companion_path(dir, &path) else { continue };
        let Ok(meta) = fs::metadata(&file) else { continue };
        if meta.len() > MAX_COMPANION_BYTES {
            kept.push(Kept { path, bytes: meta.len(), blob: None });
            continue;
        }
        let bytes = fs::read(&file).map_err(|e| StoreError::io("read", &file, e))?;
        let blob = keep_blob(dir, &bytes)?;
        kept.push(Kept { path, bytes: bytes.len() as u64, blob: Some(blob) });
    }
    Ok(kept)
}

/// What the page's own files are now, by their contents, to tell whether they changed since the
/// last version, without keeping anything.
fn companions_now(dir: &Path) -> Vec<(String, u64, Option<String>)> {
    companions(dir)
        .into_iter()
        .filter_map(|path| {
            let file = companion_path(dir, &path)?;
            let len = fs::metadata(&file).ok()?.len();
            let name = (len <= MAX_COMPANION_BYTES).then(|| fs::read(&file).ok().map(|b| content_name(&b))).flatten();
            Some((path, len, name))
        })
        .collect()
}

/// The list of the page's own files a version kept, or `None` for a version from before they
/// were kept (its words alone). One line each: the copy's name (or `-` for a file too large to
/// keep), its length, and its path in the page's folder, with tabs between. A list that cannot
/// be read, or has a line that is not one of these, is an error rather than a shorter list.
fn read_manifest(version: &Path) -> Result<Option<Vec<Kept>>> {
    let file = version.join("files.txt");
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(StoreError::io("read", &file, e)),
    };
    let mut kept = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let mut parts = line.splitn(3, '\t');
        let parsed = (|| {
            let (blob, bytes, path) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
            (!path.is_empty() && !blob.is_empty()).then(|| Kept { path: decode_path(path), bytes, blob: (blob != "-").then(|| blob.to_string()) })
        })();
        match parsed {
            Some(entry) => kept.push(entry),
            None => return Err(StoreError::invalid(format!("the list of files kept with a page version is damaged: {}", file.display()))),
        }
    }
    Ok(Some(kept))
}

/// A path as the list of kept files writes it: `%`, tabs and line breaks as `%25`, `%09`, `%0A`
/// and `%0D`, so a file name with one in it is still one line. A list written before had none of
/// these, and reads the same.
fn encode_path(path: &str) -> String {
    path.replace('%', "%25").replace('\t', "%09").replace('\n', "%0A").replace('\r', "%0D")
}

fn decode_path(path: &str) -> String {
    path.replace("%09", "\t").replace("%0A", "\n").replace("%0D", "\r").replace("%25", "%")
}

fn write_manifest(version: &Path, kept: &[Kept]) -> Result<()> {
    let text: String = kept.iter().map(|k| format!("{}\t{}\t{}\n", k.blob.as_deref().unwrap_or("-"), k.bytes, encode_path(&k.path))).collect();
    let file = version.join("files.txt");
    fs::write(&file, text).map_err(|e| StoreError::io("write", &file, e))
}

/// Takes away the copies no version names any more.
fn drop_unused_blobs(dir: &Path, versions: &[PathBuf]) {
    if !copies_are_safe(dir) {
        return;
    }
    let mut used = std::collections::HashSet::new();
    // A version still being kept (its folder there, its record not yet) may already have copies
    // its list does not name yet: none is taken away until it is done.
    let all: Vec<PathBuf> = match fs::read_dir(version_dir(dir)) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(valid_version_id))
            .collect(),
        Err(_) => return,
    };
    if all.iter().any(|v| !v.join("version.toml").is_file()) {
        return;
    }
    for version in versions.iter().chain(all.iter()).filter(|v| v.is_dir()) {
        match read_manifest(version) {
            Ok(kept) => used.extend(kept.unwrap_or_default().into_iter().filter_map(|k| k.blob)),
            // A list that cannot be read may name any copy: none is taken away.
            Err(_) => return,
        }
    }
    let Ok(entries) = fs::read_dir(blobs_dir(dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !used.contains(&name) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn version_dir(dir: &Path) -> PathBuf {
    dir.join(".notecraft").join("versions")
}

fn valid_version_id(id: &str) -> bool {
    id.len() == 20 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn snapshot_current(dir: &Path, file: &Path, force: bool) -> Result<()> {
    if !file.is_file() {
        return Ok(());
    }
    let versions = version_dir(dir);
    // The versions themselves, not the folder of copies of the page's own files beside them.
    let mut existing: Vec<PathBuf> = match fs::read_dir(&versions) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_dir() && path.file_name().and_then(|n| n.to_str()).is_some_and(valid_version_id))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(StoreError::io("read", &versions, e)),
    };
    existing.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    let current = fs::read(file).map_err(|e| StoreError::io("read", file, e))?;
    if let Some(last) = existing.last() {
        let previous = last.join(file.file_name().unwrap_or_default());
        // The same words and the same ink and attachments: nothing to keep.
        let same_files = || {
            read_manifest(last).ok().flatten().is_none_or(|kept| {
                let now = companions_now(dir);
                now.len() == kept.len()
                    && now.iter().zip(&kept).all(|((path, len, name), k)| {
                        *path == k.path && *len == k.bytes && name.as_deref().zip(k.blob.as_deref()).is_none_or(|(n, b)| b.starts_with(n))
                    })
            })
        };
        if let Ok(bytes) = fs::read(&previous)
            && bytes == current
            && same_files()
        {
            return Ok(());
        }
        let last_id = last.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let current_id = new_id();
        let recent = last_id.get(..16).and_then(|value| u128::from_str_radix(value, 16).ok());
        let now = current_id.get(..16).and_then(|value| u128::from_str_radix(value, 16).ok());
        if !force && recent.zip(now).is_some_and(|(before, after)| after.saturating_sub(before) < VERSION_INTERVAL_NS) {
            return Ok(());
        }
    }
    fs::create_dir_all(&versions).map_err(|e| StoreError::io("create", &versions, e))?;
    let id = new_id();
    let entry = versions.join(&id);
    fs::create_dir(&entry).map_err(|e| StoreError::io("create", &entry, e))?;
    let snapshot = entry.join(file.file_name().unwrap_or_default());
    if let Err(e) = fs::write(&snapshot, &current) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("write", &snapshot, e));
    }
    // The page's ink and attachments as they are, kept with its words (issue #553).
    if copies_are_safe(dir)
        && let Err(e) = keep_companions(dir).and_then(|kept| write_manifest(&entry, &kept))
    {
        let _ = fs::remove_dir_all(&entry);
        return Err(e);
    }
    let info = format!("id = {}\ncreated = {}\n", kv::quote(&id), kv::quote(&iso_now()));
    if let Err(e) = fs::write(entry.join("version.toml"), info) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("write", &entry.join("version.toml"), e));
    }
    existing.push(entry);
    // What the versions from `from` on take up: their page files, and the copies they name, each
    // copy once however many name it (issue #553). The newest is always kept.
    let page_sizes: Vec<u64> =
        existing.iter().map(|v| fs::metadata(v.join(file.file_name().unwrap_or_default())).map(|m| m.len()).unwrap_or(0)).collect();
    let names: Vec<Vec<String>> =
        existing.iter().map(|v| read_manifest(v).ok().flatten().unwrap_or_default().into_iter().filter_map(|k| k.blob).collect()).collect();
    let blob_size = |name: &String| fs::metadata(blobs_dir(dir).join(name)).map(|m| m.len()).unwrap_or(0);
    let drop = versions_to_drop(&page_sizes, &names, blob_size, MAX_VERSIONS, MAX_VERSION_BYTES);
    for version in &existing[..drop] {
        let _ = fs::remove_dir_all(version);
    }
    // Every version folder: one that could not be removed still names its copies.
    drop_unused_blobs(dir, &existing);
    Ok(())
}

/// How many of the oldest versions go, so that at most `max_versions` are left and they take up
/// no more than `max_bytes`: their page files, and the copies they name, each copy counted once
/// however many name it. The newest is always kept.
fn versions_to_drop(page_sizes: &[u64], names: &[Vec<String>], blob_size: impl Fn(&String) -> u64, max_versions: usize, max_bytes: u64) -> usize {
    let taken = |from: usize| -> u64 {
        let mut seen = std::collections::HashSet::new();
        let copies: u64 = names[from..].iter().flatten().filter(|n| seen.insert(*n)).map(&blob_size).sum();
        page_sizes[from..].iter().sum::<u64>().saturating_add(copies)
    };
    let mut next = 0;
    while next + 1 < page_sizes.len() && (page_sizes.len() - next > max_versions || taken(next) > max_bytes) {
        next += 1;
    }
    next
}

/// The editable part of a page's file: the inside of the HTML body, or the Markdown after the
/// front matter.
fn body_of(format: PageFormat, text: &str) -> String {
    match format {
        PageFormat::Html => html::inner(text),
        PageFormat::Cmgfm => frontmatter::split_front(text).1.trim_start_matches('\n').to_string(),
    }
}

/// The name the page direction goes by in both formats.
const DIR_KEY: &str = "dir";
const DIR_META: &str = "notecraft:dir";

/// The direction written in a page's own file, if any.
fn read_direction(text: &str, format: PageFormat) -> Option<Dir> {
    let raw = match format {
        PageFormat::Html => html::meta(text, DIR_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(text).0, DIR_KEY),
    }?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "rtl" => Some(Dir::Rtl),
        "ltr" => Some(Dir::Ltr),
        _ => None,
    }
}

/// Which way the whole page reads, as written in its file.
pub fn direction(dir: &Path) -> Result<Option<Dir>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(read_direction(&text, format))
}

/// Writes the page's direction into its own file, so it travels with the folder.
/// `None` takes it out again and lets every line decide for itself.
pub fn set_direction(dir: &Path, want: Option<Dir>) -> Result<()> {
    // One writer to the page at a time, as the other setters: a save keeping a version in the
    // meantime is not undone by this one's tidying of copies (issue #553).
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = match (format, want) {
        (PageFormat::Html, Some(d)) => html::set_meta(&text, DIR_META, d.id()),
        // An HTML page keeps the meta tag with an empty value rather than losing the line.
        (PageFormat::Html, None) => html::set_meta(&text, DIR_META, ""),
        (PageFormat::Cmgfm, want) => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match want {
                Some(d) => frontmatter::set_value(&front, DIR_KEY, d.id()),
                None => frontmatter::remove_value(&front, DIR_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

/// Where older versions kept the boxes: a line in the head or the front matter. Still read, so
/// an older page keeps its arrangement, and cleared the next time the boxes are written.
const BOXES_KEY: &str = "containers";
const BOXES_META: &str = "notecraft:containers";

/// The containers a page keeps. They are elements in the page's own text
/// ([`notecraft_formats::boxes`]); a page from an older version has them on a line in its head or
/// front matter instead, and that is read when the text has none. An empty list means the page
/// has never been arranged, and is one column of blocks.
pub fn containers(dir: &Path) -> Result<Vec<Container>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if let (_, Some(in_the_text)) = boxes::read(format, &body_of(format, &text)) {
        return Ok(in_the_text);
    }
    let raw = match format {
        PageFormat::Html => html::meta(&text, BOXES_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, BOXES_KEY),
    };
    Ok(raw.map(|r| container::parse(&r)).unwrap_or_default())
}

/// Arranges the page's blocks in these boxes, by writing them into the page's own text as
/// elements around the blocks. An empty list takes the boxes out again: one column. The counts
/// must add up to the page's blocks, and a page with markup the block model would drop is
/// refused rather than rewritten.
pub fn set_containers(dir: &Path, arrangement: &[Container]) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = body_of(format, &text);
    if format == PageFormat::Html && html::unsupported_tag(&body).is_some() {
        return Err(StoreError::invalid("this page has markup the block editor would lose, so its boxes cannot be written"));
    }
    let (blocks, _) = boxes::read(format, &body);
    let wanted = (!arrangement.is_empty()).then_some(arrangement);
    if let Some(wanted) = wanted {
        let counted: usize = wanted.iter().map(|c| c.blocks).sum();
        if counted != blocks.len() {
            return Err(StoreError::invalid(format!("the boxes hold {counted} blocks but the page has {}", blocks.len())));
        }
    }
    save_body(dir, &boxes::write(format, &blocks, wanted))?;
    clear_old_boxes(dir)
}

/// The page's file text with the line an older version kept the boxes on taken out of the head
/// or the front matter. Unchanged when there is no such line.
fn without_old_boxes(format: PageFormat, text: &str) -> String {
    match format {
        PageFormat::Html if html::meta(text, BOXES_META).is_some() => html::set_meta(text, BOXES_META, ""),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(text);
            if frontmatter::value(&front, BOXES_KEY).is_none() {
                return text.to_string();
            }
            format!("{}{rest}", frontmatter::remove_value(&front, BOXES_KEY))
        }
        _ => text.to_string(),
    }
}

/// Takes the older line out of the page's file.
fn clear_old_boxes(dir: &Path) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = without_old_boxes(format, &text);
    if out == text { Ok(()) } else { write_atomic(&file, &out) }
}

/// The page file's head as text: the `<head>` of an HTML page, the front matter of a
/// Markdown one. What the page records about itself, for the code view to show.
pub fn head_text(dir: &Path) -> Result<String> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(match format {
        PageFormat::Html => match text.find("<body") {
            Some(at) => text[..at].to_string(),
            None => text,
        },
        PageFormat::Cmgfm => frontmatter::split_front(&text).0.to_string(),
    })
}

/// The ink files in a page's folder, in name order.
///
/// A file is ink because its SVG carries NoteCraft's ink namespace, not because of what
/// it is called: an ordinary picture dropped in the folder is never mistaken for one, and
/// neither is the plain copy that Save Picture writes. Only the first few kilobytes of
/// each file are read.
pub fn ink_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.to_ascii_lowercase().ends_with(".svg") || !entry.path().is_file() {
            continue;
        }
        let Ok(head) = read_head(&entry.path(), 4096) else { continue };
        if notecraft_ink::svg::is_ink(&head) {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// The first `want` bytes of a file, as text. Used to look at a head without reading a
/// drawing that may be megabytes.
fn read_head(path: &Path, want: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; want];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// The paper a page is drawn on, and the colour behind it. Both are the page's own, the
/// way OneNote gives a page its rule lines and its page colour, and both live in the head
/// beside the direction.
const PAPER_KEY: &str = "paper";
const PAPER_META: &str = "notecraft:paper";
const COLOUR_KEY: &str = "page-color";
const COLOUR_META: &str = "notecraft:page-color";

/// The paper and the colour a page carries, as they are written: `(paper, colour)`. Either
/// is `None` when the page says nothing and the window's own choice stands.
pub fn appearance(dir: &Path) -> Result<(Option<String>, Option<String>)> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let read = |meta: &str, key: &str| {
        match format {
            PageFormat::Html => html::meta(&text, meta),
            PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, key),
        }
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    };
    Ok((read(PAPER_META, PAPER_KEY), read(COLOUR_META, COLOUR_KEY)))
}

/// Writes them into the page's own file. `None` takes the line out again.
pub fn set_appearance(dir: &Path, paper: Option<&str>, colour: Option<&str>) -> Result<()> {
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if read_only_in(format, &text) {
        return Err(StoreError::invalid("this page is locked read only"));
    }
    let out = match format {
        PageFormat::Html => {
            let text = html::set_meta(&text, PAPER_META, paper.unwrap_or_default());
            html::set_meta(&text, COLOUR_META, colour.unwrap_or_default())
        }
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match paper {
                Some(p) => frontmatter::set_value(&front, PAPER_KEY, p),
                None => frontmatter::remove_value(&front, PAPER_KEY),
            };
            let front = match colour {
                Some(c) => frontmatter::set_value(&front, COLOUR_KEY, c),
                None => frontmatter::remove_value(&front, COLOUR_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const RULES_KEY: &str = "rule-color";
const RULES_META: &str = "notecraft:rule-color";

/// The colour a page's rule and grid lines are drawn in (issue #288), as it is written, or
/// `None` when the page says nothing and its lines are the theme's own grey.
pub fn rule_colour(dir: &Path) -> Result<Option<String>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(match format {
        PageFormat::Html => html::meta(&text, RULES_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, RULES_KEY),
    }
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty()))
}

/// Writes it into the page's own file, beside the paper. `None` takes the line out again.
pub fn set_rule_colour(dir: &Path, colour: Option<&str>) -> Result<()> {
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if read_only_in(format, &text) {
        return Err(StoreError::invalid("this page is locked read only"));
    }
    let out = match format {
        PageFormat::Html => html::set_meta(&text, RULES_META, colour.unwrap_or_default()),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match colour {
                Some(c) => frontmatter::set_value(&front, RULES_KEY, c),
                None => frontmatter::remove_value(&front, RULES_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const PAPER_SIZE_KEY: &str = "paper_size";
const PAPER_SIZE_META: &str = "notecraft:paper-size";

/// The sheet the page is written for (issue #269). `None` is Auto: the page grows with its
/// content. A line that does not read is Auto too.
pub fn paper_size(dir: &Path) -> Result<Option<notecraft_model::paper::Paper>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let raw = match format {
        PageFormat::Html => html::meta(&text, PAPER_SIZE_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, PAPER_SIZE_KEY),
    };
    Ok(raw.and_then(|r| notecraft_model::paper::Paper::parse(&r)))
}

/// Writes it into the page's own file. `None` takes the line out, which is Auto.
pub fn set_paper_size(dir: &Path, paper: Option<notecraft_model::paper::Paper>) -> Result<()> {
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if read_only_in(format, &text) {
        return Err(StoreError::invalid("this page is locked read only"));
    }
    let line = paper.map(notecraft_model::paper::Paper::write);
    let out = match format {
        PageFormat::Html => html::set_meta(&text, PAPER_SIZE_META, line.as_deref().unwrap_or_default()),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match &line {
                Some(l) => frontmatter::set_value(&front, PAPER_SIZE_KEY, l),
                None => frontmatter::remove_value(&front, PAPER_SIZE_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const COLUMNS_KEY: &str = "columns";
const COLUMNS_META: &str = "notecraft:columns";

/// The column widths the page's tables were given. Empty means every table is drawn at
/// the usual width, which is what a page says until a column is dragged.
pub fn table_columns(dir: &Path) -> Result<tablecols::Widths> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let raw = match format {
        PageFormat::Html => html::meta(&text, COLUMNS_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, COLUMNS_KEY),
    };
    Ok(raw.map(|r| tablecols::parse(&r)).unwrap_or_default())
}

/// Writes them into the page's own file, beside the arrangement of its boxes.
pub fn set_table_columns(dir: &Path, widths: &tablecols::Widths) -> Result<()> {
    // One writer to the page at a time, as the other setters: a save keeping a version in the
    // meantime is not undone by this one's tidying of copies (issue #553).
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let line = tablecols::write(widths);
    let out = match format {
        PageFormat::Html => html::set_meta(&text, COLUMNS_META, &line),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front =
                if widths.is_empty() { frontmatter::remove_value(&front, COLUMNS_KEY) } else { frontmatter::set_value(&front, COLUMNS_KEY, &line) };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const READ_ONLY_KEY: &str = "read-only";
const READ_ONLY_META: &str = "notecraft:read-only";

/// Whether the page is locked read only (a page property, not a document property). The lock is
/// written in the page's own head, so every program that reads the file sees it.
pub fn read_only(dir: &Path) -> Result<bool> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(read_only_in(format, &text))
}

/// The lock as a page file's text says it: only the value `true` locks, so a line that is
/// missing or says anything else leaves the page editable.
pub(crate) fn read_only_in(format: PageFormat, text: &str) -> bool {
    let raw = match format {
        PageFormat::Html => html::meta(text, READ_ONLY_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(text).0, READ_ONLY_KEY),
    };
    raw.is_some_and(|v| v.trim() == "true")
}

/// Locks the page read only, or unlocks it. Unlocking removes the line, so an unlocked page
/// is the same file it was before it was locked.
pub fn set_read_only(dir: &Path, locked: bool) -> Result<()> {
    // The same writer lock as a body save, so a save in flight is not lost under this write.
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = match format {
        PageFormat::Html => {
            if locked {
                html::set_meta(&text, READ_ONLY_META, "true")
            } else {
                html::remove_meta(&text, READ_ONLY_META)
            }
        }
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = if locked { frontmatter::set_value(&front, READ_ONLY_KEY, "true") } else { frontmatter::remove_value(&front, READ_ONLY_KEY) };
            format!("{front}{rest}")
        }
    };
    // A page whose file has no head to hold the line cannot be locked: a lock that is not stored
    // would leave the page editable while it says it is locked (issue #441).
    if locked && !read_only_in(format, &out) {
        return Err(StoreError::invalid("this page has no head to hold its lock, so it cannot be locked read only"));
    }
    write_atomic(&file, &out)
}

const JUSTIFY_KEY: &str = "justify";
const JUSTIFY_META: &str = "notecraft:justify";

/// Which paragraphs of the page are justified. Empty means every paragraph is drawn ragged
/// at its far edge, which is what a page says until one is asked for.
///
/// The line names the paragraphs by their place among the page's blocks; the words of the
/// page are not touched, so the file reads the same to any other program. See
/// [`notecraft_model::justify`] and `docs/dom_serialization/page-model.md` section 2 for why it is here and
/// not in the markup.
pub fn justified(dir: &Path) -> Result<justify::Justified> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let raw = match format {
        PageFormat::Html => html::meta(&text, JUSTIFY_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, JUSTIFY_KEY),
    };
    Ok(raw.map(|r| justify::parse(&r)).unwrap_or_default())
}

/// Writes them into the page's own file, beside the table column widths.
pub fn set_justified(dir: &Path, marked: &justify::Justified) -> Result<()> {
    // One writer to the page at a time, as the other setters: a save keeping a version in the
    // meantime is not undone by this one's tidying of copies (issue #553).
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let line = justify::write(marked);
    let out = match format {
        PageFormat::Html => html::set_meta(&text, JUSTIFY_META, &line),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front =
                if marked.is_empty() { frontmatter::remove_value(&front, JUSTIFY_KEY) } else { frontmatter::set_value(&front, JUSTIFY_KEY, &line) };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

/// Saves the editable part of a page, keeping its metadata and updating `updated`.
/// Returns the new `updated` timestamp.
/// Compare and save under the same in-process write lock. A stale editor keeps its input.
pub fn save_body_checked(dir: &Path, body: &str, expected: &str) -> Result<String> {
    save_body_checked_by(dir, body, expected, None)
}

/// [`save_body_checked`], noting `author` as having changed the page when a name is given
/// (issue #292).
pub fn save_body_checked_by(dir: &Path, body: &str, expected: &str, author: Option<&str>) -> Result<String> {
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    let current = load_page(dir)?;
    if current.body != expected {
        return Err(StoreError::invalid("This page changed in another window or on disk. Keep your edits as a copy or reload the newer page."));
    }
    save_body_inner(dir, body, author)
}

pub fn save_body(dir: &Path, body: &str) -> Result<String> {
    save_body_by(dir, body, None)
}

/// [`save_body`], noting `author` as having changed the page when a name is given. The name
/// goes into the same write as the words, so the page is written once (issue #292).
pub fn save_body_by(dir: &Path, body: &str, author: Option<&str>) -> Result<String> {
    let writer = crate::coordination::page_writer(dir)?;
    let _guard = writer.lock().map_err(|_| StoreError::invalid("the page writer is unavailable"))?;
    save_body_inner(dir, body, author)
}

fn save_body_inner(dir: &Path, body: &str, author: Option<&str>) -> Result<String> {
    if body.len() > MAX_PAGE_BYTES {
        return Err(StoreError::invalid("this page is too large to save"));
    }
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    // A locked page keeps its words until it is unlocked (issue #441).
    if read_only_in(format, &text) {
        return Err(StoreError::invalid("this page is locked read only"));
    }
    let now = iso_now();
    let out = update_body(format, &text, body, &now, false);
    let out = match author {
        Some(name) => crate::authors::note(format, &out, name),
        None => out,
    };
    write_atomic(&file, &out)?;
    Ok(now)
}

/// Replaces the editable body while keeping the live page header and marking the write today.
fn update_body(format: PageFormat, text: &str, body: &str, now: &str, restore: bool) -> String {
    let out = match format {
        PageFormat::Html => html::set_meta(&html::replace_inner(text, body), "notecraft:updated", now),
        PageFormat::Cmgfm => {
            let (front, _) = frontmatter::split_front(text);
            let front = if restore {
                frontmatter::set_value(&front, "updated", now)
            } else if front.is_empty() {
                front
            } else {
                kv::replace_line(&front, "updated:", &format!("updated: {now}"))
            };
            format!("{front}\n{}", body.trim_start_matches('\n'))
        }
    };
    // A body with its boxes in the text says everything the older line said, so the line goes.
    if boxes::read(format, body).1.is_some() { without_old_boxes(format, &out) } else { out }
}

/// Writes a file by writing a temporary file next to it and renaming it over the
/// target, so a crash cannot leave half a page.
///
/// A page file larger than [`MAX_PAGE_BYTES`] is refused before anything is written: it could
/// not be opened again, and the page on disk is kept as it was. The limit is on the whole
/// file, head and all, because that is what [`load_page`] measures (issue #113).
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && crate::tree::page_file(parent).is_some_and(|(_, page)| page == path)
    {
        if text.len() > MAX_PAGE_BYTES {
            return Err(StoreError::invalid("this page is too large to save"));
        }
        snapshot_current(parent, path, false)?;
    }
    write_atomic_file(path, text)
}

fn write_atomic_file(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, text).map_err(|e| StoreError::io("write", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        StoreError::io("replace", path, e)
    })
}

#[cfg(test)]
mod tests {
    /// Issue #113: a body at the limit was saved, but the page file round it was larger than
    /// the limit, so the page could not be opened again. A save that would make the file too
    /// large is refused and the page is left as it was; one that fits opens again.
    #[test]
    fn every_page_that_saves_opens_again() {
        let t = crate::testing::TempDir::new("page-size-limit");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        for format in [notecraft_model::PageFormat::Html, notecraft_model::PageFormat::Cmgfm] {
            let page = crate::ops::create_node(&notebook, &format!("Page {}", format.id()), notecraft_model::NodeKind::Page, Some(format), &settings)
                .expect("page");
            save_body(&page, "kept").expect("a small save");
            let (_, file) = crate::tree::page_file(&page).expect("the page file");
            let before = fs::read_to_string(&file).expect("the page file");

            let full = "a".repeat(MAX_PAGE_BYTES);
            assert!(save_body(&page, &full).is_err(), "{format:?}: a body at the limit leaves no room for the page round it");
            assert_eq!(fs::read_to_string(&file).expect("the page file"), before, "{format:?}: the refused save changed nothing");
            assert_eq!(load_page(&page).expect("the kept page opens").body.trim(), "kept");

            let fits = "a".repeat(MAX_PAGE_BYTES - 64 * 1024);
            save_body(&page, &fits).expect("a body that fits with its head");
            assert_eq!(load_page(&page).expect("the saved page opens again").body.trim().len(), fits.len(), "{format:?}");
        }
    }

    /// Issue #269: a page's paper is kept in its own file in both formats, survives a save of
    /// its words, and Auto takes it out again.
    #[test]
    fn a_page_keeps_its_paper_size_in_both_formats() {
        use notecraft_model::paper::{Paper, Size};
        let t = crate::testing::TempDir::new("page-paper-size");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        for format in [notecraft_model::PageFormat::Html, notecraft_model::PageFormat::Cmgfm] {
            let page =
                crate::ops::create_node(&notebook, &format!("Sheet {}", format.id()), notecraft_model::NodeKind::Page, Some(format), &settings)
                    .expect("page");
            assert_eq!(paper_size(&page).expect("read"), None, "{format:?}: a new page is Auto");
            let letter = Paper { size: Size::Letter, landscape: true, margins: [10.0, 12.5, 10.0, 12.5] };
            set_paper_size(&page, Some(letter)).expect("set");
            save_body(&page, "words after the paper").expect("save");
            assert_eq!(paper_size(&page).expect("read"), Some(letter), "{format:?}");
            assert_eq!(load_page(&page).expect("load").body.trim(), "words after the paper", "{format:?}: the words are untouched");
            let (_, file) = crate::tree::page_file(&page).expect("file");
            assert!(fs::read_to_string(&file).expect("text").contains("letter landscape 10,12.5,10,12.5"), "{format:?}: one plain line");
            set_paper_size(&page, None).expect("auto");
            assert_eq!(paper_size(&page).expect("read"), None, "{format:?}: Auto again");
        }
    }

    #[test]
    fn read_only_blocks_all_paper_appearance_changes_in_both_formats() {
        let t = crate::testing::TempDir::new("locked-paper");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let paper = notecraft_model::paper::Paper::parse("a6 portrait").expect("paper");
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let page = crate::ops::create_node(&notebook, format.id(), notecraft_model::NodeKind::Page, Some(format), &settings).expect("page");
            set_read_only(&page, true).expect("lock");
            let (_, file) = page_file(&page).expect("file");
            let before = fs::read(&file).expect("read");
            assert!(set_appearance(&page, Some("grid-small"), Some("yellow")).is_err());
            assert!(set_rule_colour(&page, Some("red")).is_err());
            assert!(set_paper_size(&page, Some(paper)).is_err());
            assert_eq!(fs::read(&file).expect("read"), before);
            set_read_only(&page, false).expect("unlock");
            set_paper_size(&page, Some(paper)).expect("unlocked changes still work");
            assert_eq!(paper_size(&page).expect("paper size"), Some(paper));
        }
    }

    #[test]
    fn page_versions_restore_the_saved_page_file() {
        let t = crate::testing::TempDir::new("page-versions");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        let initial = load_page(&page).expect("initial page").body;

        save_body(&page, "first version").expect("first save");
        let saved_versions = versions(&page).expect("versions");
        assert_eq!(saved_versions.len(), 1);
        save_body(&page, "second version").expect("second save");
        restore_version(&page, &saved_versions[0].id).expect("restore");
        assert_eq!(load_page(&page).expect("page").body, initial);
        assert_eq!(versions(&page).expect("versions after restore").len(), 2);
    }

    #[test]
    fn restoring_a_page_version_keeps_live_metadata_and_records_a_recent_edit() {
        let t = crate::testing::TempDir::new("restore-body-only");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let section = crate::ops::create_node(&notebook, "Section", notecraft_model::NodeKind::Section, None, &settings).expect("section");
            let page = crate::ops::create_node(&section, "Original title", notecraft_model::NodeKind::Page, Some(format), &settings).expect("page");
            let (_, file) = page_file(&page).expect("file");
            let before = fs::read_to_string(&file).expect("header");
            let old = match format {
                PageFormat::Html => "<p>historical content</p>",
                PageFormat::Cmgfm => "historical content\n",
            };
            fs::write(&file, update_body(format, &before, old, "2025-01-03T08:00:00Z", true)).expect("age fixture");
            save_body(&page, "current body").expect("save");
            let version = versions(&page).expect("versions").remove(0);
            let sibling = crate::ops::create_node(&section, "Sibling", notecraft_model::NodeKind::Page, Some(format), &settings).expect("sibling");
            crate::ops::move_page(&page, false).expect("move page down");
            crate::ops::set_title(&page, "Current title").expect("rename page");
            set_direction(&page, Some(Dir::Rtl)).expect("change direction");
            let live = read_node(&page).expect("live metadata");
            assert!(live.order > read_node(&sibling).expect("sibling metadata").order);

            restore_version(&page, &version.id).expect("restore");
            let restored = load_page(&page).expect("page");
            assert_eq!(restored.body.trim(), old.trim(), "{format:?}");
            let after = read_node(&page).expect("metadata after restore");
            assert_eq!((after.title, after.id, after.order), (live.title, live.id, live.order), "{format:?}");
            assert_eq!(restored.dir, Some(Dir::Rtl), "{format:?}");
            let today = iso_now();
            assert!(
                crate::read_state::page_stamp(&page).is_some_and(|stamp| stamp.starts_with(&today[..10])),
                "{format:?}: today, not the old snapshot date"
            );
            let current = versions(&page).expect("versions");
            assert!(
                current.iter().any(|v| version_body(&page, &v.id).is_ok_and(|(_, b)| b.trim() == "current body")),
                "{format:?}: pre-restore copy"
            );
        }
    }

    /// Issue #553: a version keeps the page's ink and attachments with its words; restoring it
    /// brings them back as they were, leaves a file added since alone, and refuses a damaged
    /// copy before anything is written; a version from before files were kept says so.
    #[test]
    fn a_version_keeps_the_ink_and_attachments_with_the_words() {
        let t = crate::testing::TempDir::new("page-version-files");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        let ink = |word: &str| {
            format!(
                "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"><title>{word}</title></svg>"
            )
        };
        fs::write(page.join("ink.svg"), ink("first")).expect("ink");
        fs::create_dir_all(page.join("assets")).expect("assets");
        fs::write(page.join("assets").join("a.png"), b"one").expect("asset");
        let initial = load_page(&page).expect("initial").body;
        save_body(&page, "changed words").expect("save");
        let kept = versions(&page).expect("versions");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].files, Some(2), "the ink and the attachment are kept with the words");
        // Changed since: the ink, the attachment, and a new attachment.
        fs::write(page.join("ink.svg"), ink("second")).expect("ink");
        fs::write(page.join("assets").join("a.png"), b"two").expect("asset");
        fs::write(page.join("assets").join("b.png"), b"new").expect("new asset");
        // A damaged copy: nothing is restored.
        let blob = read_manifest(&version_dir(&page).join(&kept[0].id))
            .expect("manifest")
            .expect("a list")
            .into_iter()
            .find(|k| k.path == "assets/a.png")
            .and_then(|k| k.blob)
            .expect("kept");
        let original = fs::read(blobs_dir(&page).join(&blob)).expect("blob");
        fs::write(blobs_dir(&page).join(&blob), b"bad").expect("damage");
        assert!(restore_version(&page, &kept[0].id).is_err(), "a damaged copy is refused");
        assert_eq!(load_page(&page).expect("page").body, "changed words", "nothing was restored");
        assert_eq!(fs::read(page.join("assets").join("a.png")).expect("asset"), b"two");
        fs::write(blobs_dir(&page).join(&blob), original).expect("mend");
        restore_version(&page, &kept[0].id).expect("restore");
        assert_eq!(load_page(&page).expect("page").body, initial);
        assert!(fs::read_to_string(page.join("ink.svg")).expect("ink").contains("first"), "the ink as it was");
        assert_eq!(fs::read(page.join("assets").join("a.png")).expect("asset"), b"one", "the attachment as it was");
        assert_eq!(fs::read(page.join("assets").join("b.png")).expect("new asset"), b"new", "a file added since is left alone");
        // The page as it was before the restore is a version too, with its three files.
        let now = versions(&page).expect("versions");
        assert_eq!(now.len(), 2);
        assert_eq!(now[0].files, Some(3));
        // A version from before files were kept: its words alone.
        let legacy = version_dir(&page).join(&now[1].id).join("files.txt");
        fs::remove_file(legacy).expect("as an old version");
        assert_eq!(versions(&page).expect("versions")[1].files, None, "says it keeps the words alone");
        // The copies folder is not taken for a version.
        assert!(versions(&page).expect("versions").iter().all(|v| v.id.len() == 20));
    }

    /// Review of #553: a restore that cannot put a file back leaves the page as it was; a damaged
    /// list of kept files is an error, not a shorter list; a linked assets folder is not written
    /// through; and only the files a version kept a copy of are counted.
    #[test]
    fn a_version_restore_is_all_or_nothing() {
        let t = crate::testing::TempDir::new("page-version-rollback");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        fs::create_dir_all(page.join("assets")).expect("assets");
        fs::write(page.join("assets").join("a.png"), b"one").expect("asset");
        fs::write(page.join("assets").join("b.png"), b"bee").expect("asset");
        #[cfg(unix)]
        fs::write(page.join("assets").join("odd\tname\n%41.txt"), b"odd").expect("an odd name");
        save_body(&page, "changed words").expect("save");
        let kept = versions(&page).expect("versions");
        assert_eq!(kept[0].files, Some(if cfg!(unix) { 3 } else { 2 }), "a name with a tab or line break in it is one file");
        // b.png changed, and a.png's place is now a folder: a.png cannot go back.
        fs::write(page.join("assets").join("b.png"), b"new").expect("asset");
        fs::remove_file(page.join("assets").join("a.png")).expect("remove");
        fs::create_dir_all(page.join("assets").join("a.png").join("inside")).expect("a folder in its place");
        assert!(restore_version(&page, &kept[0].id).is_err());
        assert_eq!(load_page(&page).expect("page").body, "changed words", "the words as they were");
        assert_eq!(fs::read(page.join("assets").join("b.png")).expect("b"), b"new", "and the other file");
        let strays: Vec<_> =
            fs::read_dir(page.join("assets")).expect("assets").flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(strays.is_empty(), "no staged copy left behind");
        fs::remove_dir_all(page.join("assets").join("a.png")).expect("tidy");
        // A damaged list: an error, and the copies it may name are not thrown away.
        let list = version_dir(&page).join(&kept[0].id).join("files.txt");
        let good = fs::read_to_string(&list).expect("list");
        fs::write(&list, format!("{good}junk line\n")).expect("damage");
        assert!(restore_version(&page, &kept[0].id).is_err(), "a damaged list is refused");
        assert_eq!(versions(&page).expect("versions").iter().find(|v| v.id == kept[0].id).and_then(|v| v.files), None);
        fs::write(&list, &good).expect("mend");
        // A linked assets folder is not written through.
        #[cfg(unix)]
        {
            let elsewhere = t.path().join("elsewhere");
            fs::create_dir_all(&elsewhere).expect("elsewhere");
            fs::rename(page.join("assets"), t.path().join("moved-assets")).expect("move away");
            std::os::unix::fs::symlink(&elsewhere, page.join("assets")).expect("link");
            assert!(restore_version(&page, &kept[0].id).is_err(), "refused");
            assert!(fs::read_dir(&elsewhere).expect("elsewhere").next().is_none(), "nothing written outside the page");
        }
    }

    /// Review of #553: when the folder the copies go in is a link, no copy is kept through it,
    /// none is taken away, and the version keeps its words alone, as before.
    #[cfg(unix)]
    #[test]
    fn copies_are_not_kept_through_a_linked_folder() {
        let t = crate::testing::TempDir::new("page-version-linked-copies");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        fs::create_dir_all(page.join("assets")).expect("assets");
        fs::write(page.join("assets").join("a.png"), b"one").expect("asset");
        let elsewhere = t.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        fs::create_dir_all(version_dir(&page)).expect("versions");
        std::os::unix::fs::symlink(&elsewhere, blobs_dir(&page)).expect("link");
        save_body(&page, "changed words").expect("the page is still saved");
        let kept = versions(&page).expect("versions");
        assert_eq!(kept.len(), 1, "a version of the words");
        assert_eq!(kept[0].files, None, "and its words alone");
        assert!(fs::read_dir(&elsewhere).expect("elsewhere").next().is_none(), "nothing written through the link");
    }

    /// Review of #553: the size cap counts the copies the versions keep, each once, and the newest
    /// version is kept even when it alone is over the cap.
    #[test]
    fn the_version_cap_counts_the_copies_once() {
        let size = |n: &String| match n.as_str() {
            "big" => 60,
            "big2" => 70,
            _ => 1,
        };
        let names = |list: &[&[&str]]| list.iter().map(|v| v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        // Three versions sharing one big copy: counted once, so all fit in 100.
        assert_eq!(versions_to_drop(&[5, 5, 5], &names(&[&["big"], &["big"], &["big"]]), size, 50, 100), 0);
        // A different big copy each: the oldest goes until what is left fits.
        assert_eq!(versions_to_drop(&[5, 5, 5], &names(&[&["big"], &["big2"], &["small"]]), size, 50, 100), 1);
        // Over the cap on its own: the newest stays.
        assert_eq!(versions_to_drop(&[5, 5], &names(&[&[], &["big", "big2"]]), size, 50, 100), 1);
        // The count limit still holds.
        assert_eq!(versions_to_drop(&[1, 1, 1, 1], &names(&[&[], &[], &[], &[]]), size, 2, 100), 2);
    }

    /// Issue #278: a saved version can be read without restoring it.
    #[test]
    fn a_version_is_read_without_touching_the_page() {
        let t = crate::testing::TempDir::new("page-version-body");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        // The copy kept is of the page as it was before the first save; saves within a few
        // minutes of each other share one.
        let initial = load_page(&page).expect("initial").body;
        save_body(&page, "first version").expect("first save");
        save_body(&page, "second version").expect("second save");
        let saved = versions(&page).expect("versions");
        let (_, file) = page_file(&page).expect("file");
        let before = fs::read(&file).expect("read");
        let bodies: Vec<String> = saved.iter().map(|v| version_body(&page, &v.id).expect("body").1).collect();
        assert_eq!(bodies, [initial], "the kept copy reads back as it was");
        assert_eq!(fs::read(&file).expect("read"), before, "the page file is untouched");
        assert!(version_body(&page, "not-an-id").is_err());
        assert!(version_body(&page, "0123456789abcdef0123").is_err(), "a version that is not there");
    }

    /// A file is ink because of the namespace inside it, not because of its name.
    #[test]
    fn the_ink_files_of_a_page_are_the_ones_with_our_namespace() {
        let t = crate::testing::TempDir::new("ink-files");
        let s = crate::settings::Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = crate::ops::create_node(&nb, "P", notecraft_model::NodeKind::Page, None, &s).expect("page");

        let ours = "<?xml version=\"1.0\"?>
<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"><title>Ink</title></svg>";
        fs::write(page.join("ink.svg"), ours).expect("write");
        fs::write(page.join("ink-2.svg"), ours).expect("write");
        // A picture of someone's own, and the plain copy the Draw tab writes: neither is ink.
        fs::write(page.join("plan.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>").expect("write");
        fs::write(page.join("ink-picture.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0\"/></svg>").expect("write");
        fs::write(page.join("notes.txt"), "words").expect("write");

        assert_eq!(ink_files(&page), vec!["ink-2.svg".to_string(), "ink.svg".to_string()], "named in order, and only ours");
    }

    /// Issue #126: an ordinary SVG with a character across byte 4096 is read as a picture.
    /// The head the scan reads is cut at 4096 bytes, which split the character, and looking
    /// for the namespace in it panicked while the page was being opened.
    #[test]
    fn a_character_across_the_head_of_an_svg_does_not_stop_the_page_opening() {
        let t = crate::testing::TempDir::new("ink-files-unicode");
        let s = crate::settings::Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = crate::ops::create_node(&nb, "P", notecraft_model::NodeKind::Page, None, &s).expect("page");
        let mut svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><!--".to_string();
        svg.push_str(&"a".repeat(4095 - svg.len()));
        svg.push_str("\u{e9}--><rect width=\"1\" height=\"1\"/></svg>");
        assert!(!svg.is_char_boundary(4096), "the character crosses byte 4096");
        fs::write(page.join("ordinary.svg"), &svg).expect("write");

        assert!(ink_files(&page).is_empty(), "an ordinary picture, not ink");
        assert!(load_page(&page).is_ok());
    }

    use super::*;
    use crate::ops;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::NodeKind;

    fn setup(label: &str, format: PageFormat) -> (TempDir, std::path::PathBuf) {
        let t = TempDir::new(label);
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "Page", NodeKind::Page, Some(format), &s).expect("page");
        (t, page)
    }

    #[test]
    fn html_round_trip_keeps_the_head() {
        let (_t, page) = setup("page1", PageFormat::Html);
        let loaded = load_page(&page).expect("load");
        assert_eq!((loaded.format, loaded.title.as_str()), (PageFormat::Html, "Page"));
        // A new page's body is its date stamp; the title is in the head.
        assert!(!loaded.body.contains("<h1>") && loaded.body.contains("page-date"), "{}", loaded.body);
        save_body(&page, "<h1>Page</h1><p>Hello</p>").expect("save");
        let again = load_page(&page).expect("load");
        assert_eq!(again.body, "<h1>Page</h1><p>Hello</p>");
        let raw = fs::read_to_string(page.join("index.html")).expect("raw");
        assert!(raw.contains("<title>Page</title>") && raw.contains("name=\"notecraft:id\""));
        assert!(raw.contains("name=\"notecraft:updated\""));
    }

    #[test]
    fn markdown_round_trip_keeps_the_front_matter() {
        let (_t, page) = setup("page2", PageFormat::Cmgfm);
        let before = fs::read_to_string(page.join("index.md")).expect("raw");
        save_body(&page, "# Page\n\n- [ ] one\n").expect("save");
        let after = fs::read_to_string(page.join("index.md")).expect("raw");
        let (front_before, _) = frontmatter::split_front(&before);
        let (front_after, _) = frontmatter::split_front(&after);
        assert_eq!(frontmatter::value(&front_before, "id"), frontmatter::value(&front_after, "id"), "id is stable");
        assert_eq!(frontmatter::value(&front_before, "created"), frontmatter::value(&front_after, "created"));
        assert_eq!(load_page(&page).expect("load").body, "# Page\n\n- [ ] one\n");
    }

    /// Hidden borders are part of the line the page keeps for its tables, so they are kept
    /// for one table, in both formats, and the words of the page are not touched (issue #161).
    #[test]
    fn hidden_table_borders_survive_a_save_and_a_reopen() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("table-borders", format);
            assert!(table_columns(&dir).expect("read").is_empty(), "{format:?}: a new page has every table's borders");

            let body = match format {
                PageFormat::Html => {
                    "<table><tr><th>A</th></tr><tr><td>one</td></tr></table><p>Between</p><table><tr><th>C</th></tr><tr><td>two</td></tr></table>"
                }
                PageFormat::Cmgfm => "| A |\n|---|\n| one |\n\nBetween\n\n| C |\n|---|\n| two |\n",
            };
            save_body(&dir, body).expect("save");
            let mut layout = table_columns(&dir).expect("read");
            assert!(layout.set_borders(0, true));
            set_table_columns(&dir, &layout).expect("write");

            let back = table_columns(&dir).expect("read");
            assert!(back.borders_hidden(0), "{format:?}: the first table has none");
            assert!(!back.borders_hidden(2), "{format:?}: the second table keeps its own");
            assert_eq!(back, layout, "{format:?}");
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the body is the same text");

            // An edit of the words elsewhere does not lose it, and showing them again takes
            // the line out.
            save_body(&dir, body).expect("save");
            assert!(table_columns(&dir).expect("read").borders_hidden(0), "{format:?}: an edit does not lose it");
            let mut shown = back;
            assert!(shown.set_borders(0, false));
            set_table_columns(&dir, &shown).expect("write");
            assert!(table_columns(&dir).expect("read").is_empty(), "{format:?}");
        }
    }

    /// A cell's alignment is part of the same line, so it is kept the same way in both formats,
    /// beside widths and hidden borders, and the words of the page are not touched (issue #170).
    #[test]
    fn aligned_table_cells_survive_a_save_and_a_reopen() {
        use notecraft_model::tablecols::Align;
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("table-align", format);
            let body = match format {
                PageFormat::Html => "<table><tr><th>A</th><th>B</th></tr><tr><td>one</td><td>1</td></tr></table>",
                PageFormat::Cmgfm => "| A | B |\n|---|---|\n| one | 1 |\n",
            };
            save_body(&dir, body).expect("save");
            let mut layout = table_columns(&dir).expect("read");
            layout.set(0, 0, 200.0, &[160.0, 160.0]);
            assert!(layout.set_borders(0, true));
            assert!(layout.set_align(0, &[(0, 1), (1, 1)], Some(Align::Right)));
            assert!(layout.set_align(0, &[(1, 0)], Some(Align::Center)));
            set_table_columns(&dir, &layout).expect("write");

            let back = table_columns(&dir).expect("read");
            assert_eq!(back, layout, "{format:?}");
            assert_eq!(back.align(0, (1, 1)), Some(Align::Right), "{format:?}");
            assert_eq!(back.align(0, (1, 0)), Some(Align::Center), "{format:?}");
            assert_eq!(back.of(0), Some([200.0, 160.0].as_slice()), "{format:?}: the widths are kept beside it");
            assert!(back.borders_hidden(0), "{format:?}");
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the body is the same text");
            save_body(&dir, body).expect("save");
            assert_eq!(table_columns(&dir).expect("read"), layout, "{format:?}: an edit of the words does not lose it");
        }
    }

    /// A justified paragraph is a line in the page's head, and the words of the page are
    /// not touched by it: a page saved and reopened comes back with the same body and the
    /// same paragraphs justified, in both formats.
    #[test]
    fn a_justified_paragraph_survives_a_save_and_a_reopen() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("justified", format);
            assert!(justified(&dir).expect("read").is_empty(), "{format:?}: a new page has nothing justified");

            let body = match format {
                PageFormat::Html => "<p>One</p><p>Two</p><p>Three</p>",
                PageFormat::Cmgfm => "One\n\nTwo\n\nThree\n",
            };
            save_body(&dir, body).expect("save");
            let mut marked = justify::Justified::default();
            marked.set(1, Some(justify::Strength::High));
            set_justified(&dir, &marked).expect("write");

            assert_eq!(justified(&dir).expect("read"), marked, "{format:?}");
            assert_eq!(justified(&dir).expect("read").strength_of(1), Some(justify::Strength::High), "{format:?}: and the strength with it");
            // The words are untouched: no tatweel, no marker, nothing but what was typed.
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the body is the same text");

            // Saving the body again keeps the line, because justification is metadata.
            save_body(&dir, body).expect("save");
            assert_eq!(justified(&dir).expect("read"), marked, "{format:?}: an edit elsewhere does not lose it");

            // And taking the mark off takes the line out again.
            marked.set(1, None);
            set_justified(&dir, &marked).expect("write");
            assert!(justified(&dir).expect("read").is_empty(), "{format:?}");
            let raw = fs::read_to_string(page_file(&dir).expect("page").1).expect("raw");
            if format == PageFormat::Cmgfm {
                assert!(!raw.contains("justify:"), "the line is gone, not left empty: {raw}");
            }
        }
    }

    /// Spacing (issue #275) rides on the same line as the alignment, so it is kept in both
    /// formats, survives an edit of the words, and takes the line out when nothing is left.
    #[test]
    fn the_room_given_to_a_paragraph_survives_a_save_and_a_reopen() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("spacing", format);
            let body = match format {
                PageFormat::Html => "<p>One</p><p>Two</p><p>Three</p>",
                PageFormat::Cmgfm => "One\n\nTwo\n\nThree\n",
            };
            save_body(&dir, body).expect("save");
            let mut marked = justify::Justified::default();
            marked.set_align(1, Some(justify::Align::Center));
            marked.set_spacing(1, Some(justify::Spacing { before: 6, after: 12, line: 18 }));
            marked.set_spacing(2, Some(justify::Spacing { before: 0, after: 4, line: 0 }));
            set_justified(&dir, &marked).expect("write");

            let back = justified(&dir).expect("read");
            assert_eq!(back, marked, "{format:?}");
            assert_eq!(back.spacing_of(1), Some(justify::Spacing { before: 6, after: 12, line: 18 }), "{format:?}");
            assert_eq!(back.align_of(1), Some(justify::Align::Center), "{format:?}: the alignment beside it is kept");
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the words are not touched");
            save_body(&dir, body).expect("save");
            assert_eq!(justified(&dir).expect("read"), marked, "{format:?}: an edit elsewhere does not lose it");

            // Taking the alignment off leaves the room; taking both off takes the line out.
            marked.set_align(1, None);
            set_justified(&dir, &marked).expect("write");
            assert_eq!(justified(&dir).expect("read").spacing_of(1).map(|s| s.line), Some(18), "{format:?}");
            marked.set_spacing(1, None);
            marked.set_spacing(2, None);
            set_justified(&dir, &marked).expect("write");
            assert!(justified(&dir).expect("read").is_empty(), "{format:?}");
            if format == PageFormat::Cmgfm {
                let raw = fs::read_to_string(page_file(&dir).expect("page").1).expect("raw");
                assert!(!raw.contains("justify:"), "the line is gone, not left empty: {raw}");
            }
        }
    }

    #[test]
    fn saving_a_body_with_boxes_in_it_drops_the_older_line_and_a_plain_one_keeps_it() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("older-boxes", format);
            let file = page_file(&dir).expect("page").1;
            let text = fs::read_to_string(&file).expect("read");
            // An older page: the boxes on a line in the front matter or the head.
            let older = match format {
                PageFormat::Html => html::set_meta(&text, BOXES_META, "0,0,300,1"),
                PageFormat::Cmgfm => text.replacen("\ntitle:", "\ncontainers: 0,0,300,1\ntitle:", 1),
            };
            fs::write(&file, &older).expect("write");
            assert_eq!(containers(&dir).expect("read").len(), 1, "{format:?}: the older line is read");

            // A plain body does not touch it.
            save_body(&dir, "one\n").expect("save plain");
            let kept = fs::read_to_string(&file).expect("read");
            assert!(kept.contains("0,0,300,1"), "{format:?}: a plain save leaves the older line alone:\n{kept}");

            // A body with its boxes in it replaces it.
            let body =
                notecraft_formats::boxes::write(format, &notecraft_formats::markdown::parse("one\n"), Some(&[Container::new(10.0, 10.0, 300.0, 1)]));
            save_body(&dir, &body).expect("save boxed");
            let now = fs::read_to_string(&file).expect("read");
            assert!(!now.contains("0,0,300,1"), "{format:?}: the older line is gone:\n{now}");
            assert_eq!(
                containers(&dir).expect("read"),
                vec![Container::new(10.0, 10.0, 300.0, 1)],
                "{format:?}: the boxes in the text are what is read"
            );
        }
    }

    #[test]
    fn saving_twice_is_stable() {
        let (_t, page) = setup("page3", PageFormat::Cmgfm);
        save_body(&page, "text\n").expect("save");
        let first = fs::read_to_string(page.join("index.md")).expect("raw");
        save_body(&page, "text\n").expect("save");
        let second = fs::read_to_string(page.join("index.md")).expect("raw");
        let strip = |s: &str| s.lines().filter(|l| !l.starts_with("updated:")).collect::<Vec<_>>().join("\n");
        assert_eq!(strip(&first), strip(&second));
    }

    #[test]
    fn no_temp_file_is_left_behind() {
        let (_t, page) = setup("page4", PageFormat::Html);
        save_body(&page, "<p>x</p>").expect("save");
        let names: Vec<String> =
            fs::read_dir(&page).expect("dir").filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains("tmp-write")), "{names:?}");
    }

    #[test]
    fn a_folder_that_is_not_a_page_is_an_error() {
        let t = TempDir::new("page5");
        assert!(matches!(load_page(t.path()), Err(StoreError::NotFound(_))));
        assert!(matches!(save_body(t.path(), "x"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn unusual_text_survives() {
        let (_t, page) = setup("page6", PageFormat::Cmgfm);
        let body = "caf\u{e9} \u{1F600}\r\nline two\n\n    indented\n--- not front matter\n";
        save_body(&page, body).expect("save");
        assert_eq!(load_page(&page).expect("load").body, body);
    }

    /// A page can be locked read only. The flag is a line in the page's own head, so it travels
    /// with the page and survives a save of its words, in both formats (issue #441 request).
    #[test]
    fn a_read_only_page_keeps_its_lock_through_a_save_in_both_formats() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("read-only", format);
            assert!(!read_only(&dir).expect("read"), "{format:?}: a new page is editable");
            let body = match format {
                PageFormat::Html => "<p>One</p>",
                PageFormat::Cmgfm => {
                    "One
"
                }
            };
            save_body(&dir, body).expect("save while editable");

            set_read_only(&dir, true).expect("lock");
            assert!(read_only(&dir).expect("read"), "{format:?}: locked");
            assert!(load_page(&dir).expect("load").read_only, "{format:?}: the loaded page carries the lock");

            assert!(save_body(&dir, "Locked edit").is_err(), "{format:?}: a locked page refuses its own body save");
            assert!(read_only(&dir).expect("read"), "{format:?}: the refused save keeps the lock");
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the words are the same");

            set_read_only(&dir, false).expect("unlock");
            assert!(!read_only(&dir).expect("read"), "{format:?}: unlocked");
            assert!(!load_page(&dir).expect("load").read_only, "{format:?}: the loaded page is editable again");
            let text = std::fs::read_to_string(dir.join(match format {
                PageFormat::Html => "index.html",
                PageFormat::Cmgfm => "index.md",
            }))
            .expect("file");
            assert!(!text.contains("read-only") && !text.contains("read_only"), "{format:?}: unlocking leaves no line behind");
        }
    }

    /// A locked page is read only in every way the store can write it: its title and its
    /// words stay as they were, in both formats, until it is unlocked (issue #441).
    #[test]
    fn a_locked_page_refuses_a_new_title_and_a_new_body() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("locked-refuses", format);
            save_body(
                &dir,
                match format {
                    PageFormat::Html => "<p>Kept</p>",
                    PageFormat::Cmgfm => {
                        "Kept
"
                    }
                },
            )
            .expect("save");
            set_read_only(&dir, true).expect("lock");
            let before = std::fs::read_to_string(dir.join(match format {
                PageFormat::Html => "index.html",
                PageFormat::Cmgfm => "index.md",
            }))
            .expect("file");

            assert!(ops::set_title(&dir, "Renamed").is_err(), "{format:?}: a locked page keeps its title");
            assert!(save_body(&dir, "Changed").is_err(), "{format:?}: a locked page keeps its words");
            let after = std::fs::read_to_string(dir.join(match format {
                PageFormat::Html => "index.html",
                PageFormat::Cmgfm => "index.md",
            }))
            .expect("file");
            assert_eq!(after, before, "{format:?}: nothing was written");

            set_read_only(&dir, false).expect("unlock");
            ops::set_title(&dir, "Renamed").expect("an unlocked page takes its title again");
            save_body(&dir, "Changed").expect("and its words");
        }
    }

    /// A page whose file has no head cannot hold the lock, so locking it fails and the page stays
    /// editable, and a locked page refuses an older version over its words (issue #441).
    #[test]
    fn a_lock_must_be_stored_and_a_locked_page_keeps_its_words_through_a_restore() {
        let (_t, dir) = setup("lock-stored", PageFormat::Html);
        let file = dir.join("index.html");
        std::fs::write(&file, "<p>no head at all</p>").expect("write");
        assert!(set_read_only(&dir, true).is_err(), "a page without a head cannot be locked");
        assert!(!read_only(&dir).expect("read"), "and it stays editable");

        let (_t2, dir2) = setup("lock-restore", PageFormat::Html);
        save_body(&dir2, "<p>first</p>").expect("save");
        save_body(&dir2, "<p>second</p>").expect("save again, so a version exists");
        let versions = versions(&dir2).expect("versions");
        let id = versions.first().expect("a version").id.clone();
        set_read_only(&dir2, true).expect("lock");
        assert!(restore_version(&dir2, &id).is_err(), "a locked page is not restored over its words");
        assert_eq!(load_page(&dir2).expect("load").body, "<p>second</p>", "the words are the same");
    }
}
