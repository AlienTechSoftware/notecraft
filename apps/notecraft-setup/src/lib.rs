use std::io;
use std::path::{Path, PathBuf};

pub const NOTEBOOK_NAME: &str = "welcome-to-notecraft";

/// Finds the separate notebook folder beside the setup app on a mounted macOS DMG.
pub fn macos_dmg_payload(executable: &Path) -> Option<PathBuf> {
    let dmg_root = executable.parent()?.ancestors().nth(3)?;
    let payload = dmg_root.join("Welcome Notebook");
    payload.is_dir().then_some(payload)
}

/// Installs one generated edition (`html` or `cmgfm`) of the release Welcome notebook as the
/// app-managed notebook folder. The edition's own `_node.toml` ends up directly under the
/// notebook folder, so the notebook list finds it. Other notebooks in `notebooks_root` are never
/// visited or changed. Returns the installed notebook folder.
pub fn install_edition(payload: &Path, notebooks_root: &Path, format: &str) -> io::Result<PathBuf> {
    validate_edition(payload, format)?;
    let destination = notebooks_root.join(NOTEBOOK_NAME);
    if std::fs::symlink_metadata(&destination).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "refusing to replace a symbolic link at the Welcome notebook path"));
    }
    fs_create_dir_all(notebooks_root)?;
    let staging = notebooks_root.join(format!(".{NOTEBOOK_NAME}-installing-{}", unique_suffix()));
    let backup = notebooks_root.join(format!(".{NOTEBOOK_NAME}-backup-{}", unique_suffix()));
    fs_create_dir(&staging)?;
    if let Err(error) = copy_tree(&payload.join(format), &staging) {
        remove_tree_if_present(&staging);
        return Err(error);
    }
    let had_previous = destination.exists();
    if had_previous {
        fs_rename(&destination, &backup)?;
    }
    if let Err(error) = fs_rename(&staging, &destination) {
        if had_previous {
            let _ = fs_rename(&backup, &destination);
        }
        remove_tree_if_present(&staging);
        return Err(error);
    }
    remove_tree_if_present(&backup);
    Ok(destination)
}

/// Checks that one edition is complete before anything in the notebooks folder is replaced.
fn validate_edition(payload: &Path, format: &str) -> io::Result<()> {
    let page = match format {
        "html" => "00-start/welcome/index.html",
        "cmgfm" => "00-start/welcome/index.md",
        other => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("unknown page format {other:?}")));
        }
    };
    let format_root = payload.join(format);
    for relative in [Path::new("_node.toml").to_path_buf(), PathBuf::from(page)] {
        if !format_root.join(&relative).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the User Assistance payload is incomplete: missing {format}/{}", relative.display()),
            ));
        }
    }
    let language_count = std::fs::read_dir(&format_root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                return false;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { return false };
            let Some((number, language)) = name.split_once('-') else { return false };
            matches!(number.parse::<u8>(), Ok(10..=23)) && language.len() == 2 && language.bytes().all(|byte| byte.is_ascii_lowercase())
        })
        .count();
    if language_count != 13 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the {format} User Assistance notebook has {language_count} language folders; expected 13"),
        ));
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir(&target)?;
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the User Assistance payload contains a non-file entry"));
        }
    }
    Ok(())
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn fs_create_dir_all(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

fn fs_create_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

fn fs_rename(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

fn remove_tree_if_present(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;
    use std::fs;

    fn payload(root: &Path) -> PathBuf {
        let source = root.join("payload");
        for (format, filename, content) in [("html", "index.html", "HTML format"), ("cmgfm", "index.md", "CMGFM format")] {
            let variant = source.join(format);
            fs::create_dir_all(variant.join("00-start/welcome")).expect("create format payload");
            fs::write(variant.join("_node.toml"), format!("kind = \"notebook\"\nformat = \"{format}\"\n")).expect("notebook metadata");
            fs::write(variant.join("00-start/welcome").join(filename), content).expect("welcome page");
            for (number, language) in [
                (10, "en"),
                (11, "es"),
                (12, "fr"),
                (13, "de"),
                (14, "hi"),
                (15, "ru"),
                (16, "zh"),
                (17, "ja"),
                (18, "ko"),
                (20, "ar"),
                (21, "fa"),
                (22, "ur"),
                (23, "he"),
            ] {
                fs::create_dir_all(variant.join(format!("{number}-{language}"))).expect("create language folder");
            }
        }
        source
    }

    fn fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new("notecraft-setup");
        let source = payload(temp.path());
        let notebooks = temp.path().join("Documents/NoteCraft");
        (temp, source, notebooks)
    }

    #[test]
    fn macos_setup_app_finds_the_sibling_dmg_notebook_payload() {
        let temp = TempDir::new("notecraft-dmg-payload");
        let executable = temp.path().join("Install NoteCraft Welcome Notebook.app/Contents/MacOS/notecraft-setup");
        fs::create_dir_all(executable.parent().expect("setup executable parent")).expect("create setup bundle");
        fs::write(&executable, b"setup helper").expect("write setup helper fixture");
        let payload = temp.path().join("Welcome Notebook");
        fs::create_dir_all(&payload).expect("create DMG sibling payload");

        assert_eq!(macos_dmg_payload(&executable), Some(payload));
    }

    #[test]
    fn macos_setup_app_treats_an_omitted_dmg_payload_as_absent() {
        let temp = TempDir::new("notecraft-dmg-no-payload");
        let executable = temp.path().join("Install NoteCraft Welcome Notebook.app/Contents/MacOS/notecraft-setup");
        fs::create_dir_all(executable.parent().expect("setup executable parent")).expect("create setup bundle");
        fs::write(&executable, b"setup helper").expect("write setup helper fixture");

        assert_eq!(macos_dmg_payload(&executable), None);
    }

    #[test]
    fn official_generated_demo_is_a_valid_payload_for_both_editions() {
        let payload = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook");
        validate_edition(&payload, "html").expect("official HTML edition is valid");
        validate_edition(&payload, "cmgfm").expect("official CMGFM edition is valid");
    }

    #[test]
    fn installs_only_the_chosen_edition_at_the_notebook_root() {
        let (_temp, source, notebooks) = fixture();
        let other = notebooks.join("My notebook");
        fs::create_dir_all(&other).expect("create user notebook");
        fs::write(other.join("notes.txt"), "user content").expect("write user content");
        let destination = install_edition(&source, &notebooks, "cmgfm").expect("install CMGFM edition");

        assert_eq!(destination, notebooks.join(NOTEBOOK_NAME));
        assert!(destination.join("_node.toml").is_file(), "the edition's notebook metadata is directly under the notebook folder");
        assert_eq!(fs::read(destination.join("00-start/welcome/index.md")).expect("CMGFM format"), b"CMGFM format");
        assert!(!destination.join("html").exists(), "the other edition is not installed");
        assert_eq!(fs::read(other.join("notes.txt")).expect("user content"), b"user content");
    }

    #[test]
    fn an_existing_demo_is_replaced_but_sibling_notebooks_are_untouched() {
        let (_temp, source, notebooks) = fixture();
        let destination = notebooks.join(NOTEBOOK_NAME);
        fs::create_dir_all(destination.join("00-start/welcome")).expect("create existing demo");
        fs::write(destination.join("old-page.txt"), "old version").expect("write old demo");
        let other = notebooks.join("Work");
        fs::create_dir_all(&other).expect("create user notebook");
        fs::write(other.join("notes.txt"), "keep my notes").expect("write user notes");

        fs::write(source.join("html/00-start/welcome/index.html"), "updated HTML").expect("update payload");
        assert_eq!(install_edition(&source, &notebooks, "html").expect("replace existing demo"), destination);
        assert!(!destination.join("old-page.txt").exists(), "stale demo data is removed");
        assert_eq!(fs::read(destination.join("00-start/welcome/index.html")).expect("new demo"), b"updated HTML");
        assert_eq!(fs::read(other.join("notes.txt")).expect("user notes"), b"keep my notes");
    }

    #[test]
    fn a_missing_demo_is_recreated_on_next_install() {
        let (_temp, source, notebooks) = fixture();
        let destination = install_edition(&source, &notebooks, "html").expect("initial install");
        fs::remove_dir_all(&destination).expect("user deletes the notebook");
        assert_eq!(install_edition(&source, &notebooks, "html").expect("reinstall deleted demo"), destination);
        assert!(destination.join("00-start/welcome/index.html").is_file(), "the app recreates its managed demo");
    }

    #[test]
    fn an_incomplete_edition_fails_without_replacing_the_existing_demo() {
        let (_temp, source, notebooks) = fixture();
        fs::create_dir_all(notebooks.join(NOTEBOOK_NAME)).expect("create current demo");
        fs::write(notebooks.join(NOTEBOOK_NAME).join("keep.txt"), "current").expect("write current demo");
        fs::remove_file(source.join("html/00-start/welcome/index.html")).expect("remove required HTML page");
        assert_eq!(install_edition(&source, &notebooks, "html").expect_err("reject incomplete payload").kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(notebooks.join(NOTEBOOK_NAME).join("keep.txt")).expect("current demo"), b"current");
    }

    #[test]
    fn an_edition_missing_a_language_is_rejected_before_replacing_the_current_demo() {
        let (_temp, source, notebooks) = fixture();
        let destination = notebooks.join(NOTEBOOK_NAME);
        fs::create_dir_all(&destination).expect("create current demo");
        fs::write(destination.join("keep.txt"), "current").expect("write current demo");
        fs::remove_dir_all(source.join("html/23-he")).expect("remove one generated language");

        assert_eq!(install_edition(&source, &notebooks, "html").expect_err("reject incomplete languages").kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(destination.join("keep.txt")).expect("current demo"), b"current");
    }

    #[test]
    fn an_unknown_format_is_rejected_before_anything_is_written() {
        let (_temp, source, notebooks) = fixture();
        assert_eq!(install_edition(&source, &notebooks, "docx").expect_err("reject unknown format").kind(), io::ErrorKind::InvalidInput);
        assert!(!notebooks.join(NOTEBOOK_NAME).exists());
    }
}
