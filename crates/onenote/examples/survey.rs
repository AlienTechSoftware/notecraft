//! Structure-only survey of a folder of `.one` files: counts, timings and error kinds, never names
//! or content. Usage: `cargo run --release -p notecraft-onenote --example survey -- <folder>`.

use notecraft_onenote::{Content, OneError, read, section};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("one")) {
            out.push(p);
        }
    }
}

fn refs(store: &notecraft_onenote::Store, data: &[u8], blocks: &[Content], h: &mut BTreeMap<String, usize>) {
    for b in blocks {
        let (file, ext) = match b {
            Content::Picture { file, ext, .. } | Content::File { file, ext, .. } => (file, ext),
            Content::Table { rows } => {
                for cell in rows.iter().flatten() {
                    refs(store, data, cell, h);
                }
                continue;
            }
            _ => continue,
        };
        let kind = match file {
            None => "no reference".to_string(),
            Some(r) => {
                let prefix: String = r.chars().take_while(|c| *c != '{' && *c != '>').chain(std::iter::once('>')).collect();
                let found = store.file_bytes(data, r).is_some();
                format!("{prefix} found={found} ext={ext}")
            }
        };
        *h.entry(kind).or_default() += 1;
    }
}

fn count(blocks: &[Content], t: &mut [usize; 5]) {
    for b in blocks {
        match b {
            Content::Text { runs, .. } => {
                t[0] += 1;
                t[4] += runs.iter().map(|r| r.text.chars().count()).sum::<usize>();
            }
            Content::Picture { file, .. } => {
                t[1] += 1;
                if file.is_none() {
                    t[3] += 1;
                }
            }
            Content::File { file, .. } => {
                t[2] += 1;
                if file.is_none() {
                    t[3] += 1;
                }
            }
            Content::Table { rows } => {
                for row in rows {
                    for cell in row {
                        count(cell, t);
                    }
                }
            }
        }
    }
}

fn main() {
    let Some(root) = std::env::args().nth(1) else { return };
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    let (mut ok, mut empty) = (0usize, 0usize);
    let mut errs: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = [0usize; 5];
    let (mut pages, mut deep) = (0usize, 0usize);
    let mut slowest = (0u128, 0u64);
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        let Ok(data) = std::fs::read(f) else {
            *errs.entry("io".into()).or_default() += 1;
            continue;
        };
        let t0 = Instant::now();
        let r = read(&data);
        match r {
            Ok(store) => {
                let s = section(&store);
                let ms = t0.elapsed().as_millis();
                if ms > slowest.0 {
                    slowest = (ms, data.len() as u64);
                }
                ok += 1;
                if s.pages.is_empty() {
                    empty += 1;
                    let root = store.spaces.first();
                    let kind = root.and_then(|r| r.content_root()).map(|o| format!("{:#x}", o.kind())).unwrap_or_else(|| "none".into());
                    let enc = store.spaces.iter().filter(|x| x.encrypted).count();
                    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
                    for sp in &store.spaces {
                        if let Some(o) = sp.content_root() {
                            *kinds.entry(format!("{:#x}", o.kind())).or_default() += 1;
                        }
                    }
                    println!("   nopages: spaces={} rootkind={kind} encrypted={enc} space-root-kinds={kinds:?}", store.spaces.len());
                }
                let mut t = [0usize; 5];
                for p in &s.pages {
                    refs(&store, &data, &p.blocks, &mut hist);
                    pages += 1;
                    if p.level > 1 {
                        deep += 1;
                    }
                    count(&p.blocks, &mut t);
                }
                for k in 0..5 {
                    total[k] += t[k];
                }
                println!(
                    "#{i} {} B ok pages={} text={} pic={} file={} nodata={} chars={} {ms}ms",
                    data.len(),
                    s.pages.len(),
                    t[0],
                    t[1],
                    t[2],
                    t[3],
                    t[4]
                );
            }
            Err(e) => {
                let kind = match e {
                    OneError::NotASection(_) => "not-a-section",
                    OneError::NotesetFile => "toc",
                    OneError::Truncated(w) => w,
                    OneError::Corrupt(_) => "corrupt",
                    OneError::Encrypted => "encrypted",
                    OneError::TooLarge(_) => "too-large",
                };
                *errs.entry(kind.to_string()).or_default() += 1;
                let msg: String = e.to_string().chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect();
                println!("#{i} {} B ERR {kind}: {msg}", data.len());
            }
        }
    }
    println!("--- files={} ok={ok} ok-but-no-pages={empty} pages={pages} subpages={deep}", files.len());
    println!("--- text={} pictures={} files={} missing-data={} chars={}", total[0], total[1], total[2], total[3], total[4]);
    for (k, v) in &hist {
        println!("--- ref {k}: {v}");
    }
    println!("--- slowest={}ms on a {} B file", slowest.0, slowest.1);
    for (k, v) in errs {
        println!("--- error {k}: {v}");
    }
}
