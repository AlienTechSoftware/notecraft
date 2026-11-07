use notecraft_store::Settings;
use std::io;
use std::path::{Path, PathBuf};

const SETUP_MARKER: &str = "notecraft-setup-complete";
const NOTEBOOK_NAME: &str = "welcome-to-notecraft";

/// Finds the separate notebook folder beside the setup app on a mounted macOS DMG.
pub fn macos_dmg_payload(executable: &Path) -> Option<PathBuf> {
    let dmg_root = executable.parent()?.ancestors().nth(3)?;
    let payload = dmg_root.join("Welcome Notebook");
    payload.is_dir().then_some(payload)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupOutcome {
    Installed(PathBuf),
    Existing(PathBuf),
    AlreadyConfigured,
    NoPayload,
}

pub fn install_if_needed(settings: &Settings, payload: Option<&Path>, notebooks_root: &Path) -> io::Result<SetupOutcome> {
    if marker_path(settings).is_file() {
        return Ok(SetupOutcome::AlreadyConfigured);
    }

    let Some(payload) = payload else {
        return Ok(SetupOutcome::NoPayload);
    };
    validate_payload(payload)?;

    let destination = notebooks_root.join(NOTEBOOK_NAME);
    if destination.exists() {
        mark_configured(settings)?;
        return Ok(SetupOutcome::Existing(destination));
    }

    fs_create_dir_all(notebooks_root)?;
    let staging = notebooks_root.join(format!(".{NOTEBOOK_NAME}-installing-{}", unique_suffix()));
    fs_create_dir(&staging)?;
    let result = copy_tree(payload, &staging).and_then(|()| {
        if destination.exists() {
            return Ok(SetupOutcome::Existing(destination.clone()));
        }
        match fs_rename(&staging, &destination) {
            Ok(()) => Ok(SetupOutcome::Installed(destination.clone())),
            Err(_error) if destination.exists() => Ok(SetupOutcome::Existing(destination.clone())),
            Err(error) => Err(error),
        }
    });
    if result.is_err() {
        remove_tree_if_present(&staging);
    }
    let outcome = result?;
    remove_tree_if_present(&staging);
    mark_configured(settings)?;
    Ok(outcome)
}

fn validate_payload(payload: &Path) -> io::Result<()> {
    for relative in [
        Path::new("html/_node.toml"),
        Path::new("cmgfm/_node.toml"),
        Path::new("html/00-start/welcome/index.html"),
        Path::new("cmgfm/00-start/welcome/index.md"),
    ] {
        if !payload.join(relative).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the User Assistance payload is incomplete: missing {}", relative.display()),
            ));
        }
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

fn marker_path(settings: &Settings) -> PathBuf {
    settings.path().with_file_name(SETUP_MARKER)
}

fn mark_configured(settings: &Settings) -> io::Result<()> {
    let marker = marker_path(settings);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(marker, b"User Assistance setup completed.\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;
    use std::fs;

    fn payload(root: &Path) -> PathBuf {
        let source = root.join("payload");
        fs::create_dir_all(source.join("html/00-start/welcome")).expect("create HTML payload");
        fs::create_dir_all(source.join("cmgfm/00-start/welcome")).expect("create CMGFM payload");
        fs::write(source.join("html/_node.toml"), "kind = \"notebook\"\nformat = \"html\"\n").expect("HTML notebook metadata");
        fs::write(source.join("cmgfm/_node.toml"), "kind = \"notebook\"\nformat = \"cmgfm\"\n").expect("CMGFM notebook metadata");
        fs::write(source.join("html/00-start/welcome/index.html"), "authored HTML").expect("HTML page");
        fs::write(source.join("cmgfm/00-start/welcome/index.md"), "authored CMGFM").expect("CMGFM page");
        source
    }

    fn fixture() -> (TempDir, Settings, PathBuf, PathBuf) {
        let temp = TempDir::new("notecraft-setup");
        let source = payload(temp.path());
        let settings = Settings::at(temp.path().join("settings.toml"));
        let notebooks = temp.path().join("Documents/NoteCraft");
        (temp, settings, source, notebooks)
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
    fn official_generated_demo_is_a_valid_setup_payload() {
        let payload = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook");
        validate_payload(&payload).expect("official generated notebook has both supported formats");
    }

    #[test]
    fn installs_the_separate_payload_into_the_normal_notebooks_folder() {
        let (_temp, settings, source, notebooks) = fixture();
        let outcome = install_if_needed(&settings, Some(&source), &notebooks).expect("install payload");
        let destination = notebooks.join(NOTEBOOK_NAME);
        assert_eq!(outcome, SetupOutcome::Installed(destination.clone()));
        assert_eq!(fs::read(destination.join("html/00-start/welcome/index.html")).expect("HTML"), b"authored HTML");
        assert_eq!(fs::read(destination.join("cmgfm/00-start/welcome/index.md")).expect("CMGFM"), b"authored CMGFM");
    }

    #[test]
    fn an_existing_notebook_is_preserved_and_marked_configured() {
        let (_temp, settings, source, notebooks) = fixture();
        let destination = notebooks.join(NOTEBOOK_NAME);
        fs::create_dir_all(&destination).expect("create existing notebook");
        fs::write(destination.join("user-edit.txt"), "keep my edit").expect("write user edit");
        let outcome = install_if_needed(&settings, Some(&source), &notebooks).expect("keep existing notebook");
        assert_eq!(outcome, SetupOutcome::Existing(destination.clone()));
        assert_eq!(fs::read(destination.join("user-edit.txt")).expect("user edit"), b"keep my edit");
        assert!(marker_path(&settings).is_file());
    }

    #[test]
    fn a_user_deletion_is_not_repaired_after_setup_was_completed() {
        let (_temp, settings, source, notebooks) = fixture();
        let destination = notebooks.join(NOTEBOOK_NAME);
        assert_eq!(install_if_needed(&settings, Some(&source), &notebooks).expect("initial install"), SetupOutcome::Installed(destination.clone()));
        fs::remove_dir_all(&destination).expect("user deletes the notebook");
        settings.reset().expect("reset app preferences");
        assert_eq!(install_if_needed(&settings, Some(&source), &notebooks).expect("do not repair deletion"), SetupOutcome::AlreadyConfigured);
        assert!(!destination.exists(), "the app installer must not recreate user-deleted content");
    }

    #[test]
    fn an_omitted_payload_is_a_normal_no_demo_setup() {
        let (_temp, settings, _source, notebooks) = fixture();
        assert_eq!(install_if_needed(&settings, None, &notebooks).expect("optional payload may be omitted"), SetupOutcome::NoPayload);
        assert!(!notebooks.exists(), "missing optional payload creates no notebook root");
    }

    #[test]
    fn an_incomplete_payload_fails_without_leaving_a_partial_notebook() {
        let (_temp, settings, source, notebooks) = fixture();
        fs::remove_file(source.join("cmgfm/00-start/welcome/index.md")).expect("remove required format page");
        assert_eq!(
            install_if_needed(&settings, Some(&source), &notebooks).expect_err("reject incomplete payload").kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!notebooks.join(NOTEBOOK_NAME).exists());
        assert_eq!(settings.get(SETUP_MARKER), None);
    }
}
