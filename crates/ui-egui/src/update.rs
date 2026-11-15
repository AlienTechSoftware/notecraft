//! File > About and File > Options: "Check for updates" (issue #253).
//!
//! Nothing here runs until the button is pressed. Then NoteCraft asks GitHub for the newest
//! release, and if it is newer than this copy, downloads the file made for this computer,
//! checks it against the SHA-256 the release publishes, and puts it in place:
//!
//! - Windows, installed: the MSI runs with a progress bar only, then NoteCraft starts again.
//! - Windows, portable: the zip is unpacked and copied over this folder, then NoteCraft starts
//!   again.
//! - Linux, AppImage: the new AppImage replaces this one, then it starts.
//! - Linux, deb or rpm: the package is handed to the system's software installer.
//! - macOS: the app bundle is replaced from the disk image, then it starts. If that is not
//!   allowed, the disk image is opened instead.
//!
//! The steps that replace files run in a small script that waits for this window to close
//! first, so nothing is overwritten while it is in use. A file that does not match its
//! published checksum is never run.

use std::path::{Path, PathBuf};

/// The newest published release, as GitHub's API describes it.
pub const RELEASES_API: &str = "https://api.github.com/repos/AlienTechSoftware/notecraft/releases/latest";
/// Every file NoteCraft downloads comes from under here: NoteCraft's own releases on GitHub.
pub const DOWNLOADS: &str = "https://github.com/AlienTechSoftware/notecraft/releases/download/";

/// Whether a file's address is one of NoteCraft's own release downloads. Anything else in the
/// release answer is never fetched.
pub fn is_release_download(url: &str) -> bool {
    url.strip_prefix(DOWNLOADS).is_some_and(|rest| !rest.is_empty() && !rest.contains("..") && !rest.contains(['?', '#', '\\']))
}

/// What the updater is doing, for the File screen to show.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Status {
    /// The button has not been pressed.
    #[default]
    Idle,
    Checking,
    /// This copy is the newest release (or newer).
    UpToDate {
        latest: String,
    },
    Downloading {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    Verifying {
        version: String,
    },
    /// The new version is ready; the window closes so it can be put in place.
    Restart {
        version: String,
    },
    /// The file was handed to the system (a package installer, a disk image); nothing more to do here.
    Handed {
        version: String,
        file: PathBuf,
    },
    /// A newer release exists but has no file for this computer, or this build cannot install one.
    Manual {
        version: String,
    },
    Failed(String),
}

impl Status {
    /// True while a check or download is running, so the button is not pressed twice.
    pub fn busy(&self) -> bool {
        matches!(self, Status::Checking | Status::Downloading { .. } | Status::Verifying { .. } | Status::Restart { .. })
    }
}

/// One file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
    /// Lower-case hex, when GitHub gives it with the file.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: semver::Version,
    /// The release's page, for when it has to be installed by hand.
    pub page: String,
    pub assets: Vec<Asset>,
}

/// Where the newest release is read from. GitHub today; the shared `craft-update` crate can take
/// its place later without the File screen changing (ADR 0002).
pub trait ReleaseSource {
    fn latest(&self) -> Result<Release, String>;
}

/// A release tag as a version: `v0.0.12` and `0.0.12` are both 0.0.12.
pub fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches('v')).ok()
}

/// Whether `latest` is newer than the running version. A running version that does not parse
/// is treated as older, so the user is not left on it.
pub fn is_newer(latest: &semver::Version, running: &str) -> bool {
    parse_version(running).is_none_or(|r| *latest > r)
}

/// The release in GitHub's `releases/latest` answer.
pub fn parse_release(json: &serde_json::Value) -> Option<Release> {
    let version = parse_version(json.get("tag_name")?.as_str()?)?;
    let page = json.get("html_url").and_then(|u| u.as_str()).unwrap_or(crate::backstage::LATEST_RELEASE).to_owned();
    let assets = json
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?.to_owned();
                    let url = a.get("browser_download_url")?.as_str().filter(|u| is_release_download(u))?.to_owned();
                    let size = a.get("size").and_then(|s| s.as_u64());
                    let sha256 = a
                        .get("digest")
                        .and_then(|d| d.as_str())
                        .and_then(|d| d.strip_prefix("sha256:"))
                        .filter(|h| is_hex_digest(h))
                        .map(str::to_ascii_lowercase);
                    Some(Asset { name, url, size, sha256 })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release { version, page, assets })
}

fn is_hex_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// How this copy of NoteCraft was installed, which decides the file to download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Install {
    WindowsMsi,
    WindowsPortable,
    LinuxAppImage,
    LinuxDeb,
    LinuxRpm,
    MacApp,
    /// Built from source, a tarball, the web or Android: the releases page is the way.
    Other,
}

/// Facts about the computer that decide [`Install`], gathered by [`Install::detect`].
#[derive(Clone, Debug, Default)]
pub struct Machine {
    pub os: &'static str,
    pub exe: PathBuf,
    /// `$APPIMAGE`, set by the AppImage runtime to the AppImage file itself.
    pub appimage: Option<PathBuf>,
    /// Windows' Program Files folders.
    pub program_files: Vec<PathBuf>,
    pub debian: bool,
    pub redhat: bool,
}

impl Install {
    pub fn detect() -> Install {
        let program_files = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
        let machine = Machine {
            os: std::env::consts::OS,
            exe: std::env::current_exe().unwrap_or_default(),
            appimage: std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()),
            program_files,
            debian: Path::new("/etc/debian_version").exists(),
            redhat: Path::new("/etc/redhat-release").exists() || Path::new("/etc/fedora-release").exists() || Path::new("/etc/SuSE-release").exists(),
        };
        Install::of(&machine)
    }

    /// The install kind for the given facts.
    pub fn of(m: &Machine) -> Install {
        match m.os {
            "windows" => {
                if m.program_files.iter().any(|p| !p.as_os_str().is_empty() && starts_with_ignore_case(&m.exe, p)) {
                    Install::WindowsMsi
                } else {
                    Install::WindowsPortable
                }
            }
            "linux" if m.appimage.is_some() => Install::LinuxAppImage,
            // The deb and the rpm put the program in /usr/bin; a tarball or a build is elsewhere.
            "linux" if m.exe.starts_with("/usr/bin") && m.debian => Install::LinuxDeb,
            "linux" if m.exe.starts_with("/usr/bin") && m.redhat => Install::LinuxRpm,
            "macos" if bundle_of(&m.exe).is_some() => Install::MacApp,
            _ => Install::Other,
        }
    }

    /// The end of the release file's name for this install, after `<version>-`.
    fn suffix(self, arch: &str) -> Option<String> {
        let linux_arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return None,
        };
        Some(match self {
            Install::WindowsMsi if arch == "x86_64" => "windows-x64.msi".into(),
            Install::WindowsPortable if arch == "x86_64" => "windows-x64-portable.zip".into(),
            Install::LinuxAppImage => format!("linux-{linux_arch}.AppImage"),
            Install::LinuxDeb => format!("linux-{linux_arch}.deb"),
            Install::LinuxRpm => format!("linux-{linux_arch}.rpm"),
            Install::MacApp => format!("macos-{linux_arch}.dmg"),
            _ => return None,
        })
    }
}

fn starts_with_ignore_case(path: &Path, prefix: &Path) -> bool {
    let path = path.to_string_lossy().to_lowercase().replace('/', "\\");
    let prefix = prefix.to_string_lossy().to_lowercase().replace('/', "\\");
    let prefix = prefix.trim_end_matches('\\');
    path.starts_with(&format!("{prefix}\\"))
}

/// The `.app` folder an executable sits in: `/Applications/NoteCraft.app/Contents/MacOS/notecraft`
/// gives `/Applications/NoteCraft.app`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// The release file for this install and processor, if the release has one.
pub fn asset_for<'a>(install: Install, arch: &str, release: &'a Release) -> Option<&'a Asset> {
    let suffix = format!("{}-{}", release.version, install.suffix(arch)?);
    release.assets.iter().find(|a| a.name.ends_with(&suffix))
}

/// The SHA-256 a `SHA256SUMS.txt` gives for `name` (the `sha256sum` format: digest, two
/// spaces or a space and a star, then the file name).
pub fn sha256_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, rest) = line.trim().split_once(char::is_whitespace)?;
        let file = rest.trim_start().trim_start_matches('*');
        (file == name && is_hex_digest(digest)).then(|| digest.to_ascii_lowercase())
    })
}

/// The SHA-256 of a file, in lower-case hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Whether a file's SHA-256 is `expected` (lower-case hex).
pub fn matches_digest(path: &Path, expected: &str) -> bool {
    sha256_file(path).is_ok_and(|actual| actual == expected)
}

/// Unpacks a zip into `dest`, skipping any entry whose name would land outside it.
pub fn unzip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else { continue };
        let out = dest.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut target = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The folder a portable zip unpacked into: its single top-level folder, or `dest` itself.
pub fn unpacked_root(dest: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = std::fs::read_dir(dest).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dest.to_path_buf(),
    }
}

/// Where downloads go: a folder only this user can write to. Windows and macOS give each user
/// a temporary folder of their own; Linux's `/tmp` is shared, so the user's cache is used there.
pub fn download_root(os: &str, temp: &Path, xdg_cache: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    let base = match os {
        "linux" => xdg_cache
            .filter(|p| p.is_absolute())
            .or_else(|| home.filter(|p| p.is_absolute()).map(|h| h.join(".cache")))
            .unwrap_or_else(|| temp.to_path_buf()),
        _ => temp.to_path_buf(),
    };
    base.join("notecraft-update")
}

/// A path written into a POSIX shell script, quoted so spaces and quotes stay part of it.
pub fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// A path written into a Windows batch file: in double quotes, with `%` doubled so it is not
/// read as a variable. Windows paths cannot contain `"`.
pub fn cmd_quote(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('%', "%%"))
}

/// The lines that wait until process `pid` (this window) has gone.
fn sh_wait(pid: u32) -> String {
    format!("while kill -0 {pid} 2>/dev/null; do sleep 0.5; done\n")
}

fn cmd_wait(pid: u32) -> String {
    format!(
        ":wait\r\ntasklist /FI \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul\r\nif not errorlevel 1 (\r\n  timeout /t 1 /nobreak >nul\r\n  goto wait\r\n)\r\n"
    )
}

/// The script that puts a new AppImage in place of the running one and starts it.
pub fn appimage_script(pid: u32, new: &Path, target: &Path) -> String {
    let (new, target) = (sh_quote(new), sh_quote(target));
    format!("#!/bin/sh\n{}chmod 755 {new} && mv -f {new} {target}\nexec {target}\n", sh_wait(pid))
}

/// The script that replaces a macOS app bundle from a disk image and starts it, or opens the
/// disk image when the bundle cannot be replaced (for example without the rights to).
pub fn mac_script(pid: u32, dmg: &Path, bundle: &Path) -> String {
    let (dmg, app) = (sh_quote(dmg), sh_quote(bundle));
    let (new, old) = (sh_quote(&bundle.with_extension("app.new")), sh_quote(&bundle.with_extension("app.old")));
    format!(
        "#!/bin/sh\n{wait}mnt=\"$(mktemp -d)\" || {{ open {dmg}; exit 0; }}\n\
         if ! hdiutil attach -nobrowse -readonly -mountpoint \"$mnt\" {dmg} >/dev/null; then open {dmg}; exit 0; fi\n\
         src=\"$(find \"$mnt\" -maxdepth 1 -name '*.app' | head -n 1)\"\n\
         rm -rf {new} {old}\n\
         if [ -n \"$src\" ] && ditto \"$src\" {new} && mv {app} {old} && mv {new} {app}; then\n\
         \x20 rm -rf {old}\n\
         \x20 hdiutil detach \"$mnt\" -quiet\n\
         \x20 open {app}\n\
         else\n\
         \x20 [ -d {old} ] && [ ! -d {app} ] && mv {old} {app}\n\
         \x20 rm -rf {new}\n\
         \x20 hdiutil detach \"$mnt\" -quiet\n\
         \x20 open {dmg}\n\
         fi\n",
        wait = sh_wait(pid)
    )
}

/// The batch file that runs the MSI with a progress bar only, then starts NoteCraft again.
pub fn msi_script(pid: u32, msi: &Path, exe: &Path) -> String {
    format!("@echo off\r\n{}msiexec /i {} /passive /norestart\r\nstart \"\" {}\r\n", cmd_wait(pid), cmd_quote(msi), cmd_quote(exe))
}

/// The batch file that copies an unpacked portable zip over this folder, then starts NoteCraft again.
pub fn portable_script(pid: u32, unpacked: &Path, folder: &Path, exe: &Path) -> String {
    format!(
        "@echo off\r\n{}robocopy {} {} /E /IS /IT /NFL /NDL /NJH /NJS /NP >nul\r\nstart \"\" {}\r\n",
        cmd_wait(pid),
        cmd_quote(unpacked),
        cmd_quote(folder),
        cmd_quote(exe)
    )
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod net {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn set(status: &Arc<Mutex<Status>>, ctx: &egui::Context, value: Status) {
        if let Ok(mut s) = status.lock() {
            *s = value;
        }
        ctx.request_repaint();
    }

    fn agent() -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .user_agent(format!("NoteCraft/{}", crate::backstage::VERSION))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            // The system's own certificates, so a network behind a company proxy that the
            // computer trusts works too.
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
            .build();
        ureq::Agent::new_with_config(config)
    }

    /// Checks, downloads and prepares the update on a thread of its own.
    pub fn start(ctx: egui::Context, status: Arc<Mutex<Status>>) {
        if status.lock().is_ok_and(|s| s.busy()) {
            return;
        }
        set(&status, &ctx, Status::Checking);
        let spawned = std::thread::Builder::new().name("notecraft-update".into()).spawn({
            let (ctx, status) = (ctx.clone(), status.clone());
            move || {
                let end = run(&ctx, &status).unwrap_or_else(|e| {
                    log::warn!("update: {e}");
                    Status::Failed(e)
                });
                set(&status, &ctx, end);
            }
        });
        if let Err(e) = spawned {
            set(&status, &ctx, Status::Failed(e.to_string()));
        }
    }

    /// The newest non-draft, non-prerelease release on GitHub.
    pub struct GitHub(pub ureq::Agent);

    impl ReleaseSource for GitHub {
        fn latest(&self) -> Result<Release, String> {
            let json: serde_json::Value = self
                .0
                .get(RELEASES_API)
                .header("Accept", "application/vnd.github+json")
                .call()
                .map_err(|e| e.to_string())?
                .body_mut()
                .read_json()
                .map_err(|e| e.to_string())?;
            parse_release(&json).ok_or_else(|| "the release information could not be read".to_owned())
        }
    }

    fn run(ctx: &egui::Context, status: &Arc<Mutex<Status>>) -> Result<Status, String> {
        let agent = agent();
        let release = GitHub(agent.clone()).latest()?;
        let version = release.version.to_string();
        log::info!("update: newest release {version}, running {}", crate::backstage::VERSION);
        if !is_newer(&release.version, crate::backstage::VERSION) {
            return Ok(Status::UpToDate { latest: version });
        }
        let install = Install::detect();
        let Some(asset) = asset_for(install, std::env::consts::ARCH, &release) else {
            return Ok(Status::Manual { version });
        };
        let expected = match &asset.sha256 {
            Some(h) => h.clone(),
            None => {
                let sums = release.assets.iter().find(|a| a.name == "SHA256SUMS.txt").ok_or("the release has no checksums")?;
                // `parse_release` keeps only NoteCraft's own downloads; checked again before fetching.
                if !is_release_download(&sums.url) {
                    return Err("the checksum list is not one of NoteCraft's release files".into());
                }
                let text = agent
                    .get(&sums.url)
                    .call()
                    .map_err(|e| e.to_string())?
                    .body_mut()
                    .with_config()
                    .limit(1 << 20)
                    .read_to_string()
                    .map_err(|e| e.to_string())?;
                sha256_from_sums(&text, &asset.name).ok_or("the release has no checksum for this file")?
            }
        };
        let root = download_root(
            std::env::consts::OS,
            &std::env::temp_dir(),
            std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
            std::env::var_os("HOME").map(PathBuf::from),
        );
        let dir = root.join(&version);
        // A fresh folder each time, so nothing left there earlier (or put there by someone
        // else) is used.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for d in [&root, &dir] {
                std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
            }
        }
        let file = dir.join(&asset.name);
        download(&agent, asset, &file, ctx, status, &version)?;
        set(status, ctx, Status::Verifying { version: version.clone() });
        if !matches_digest(&file, &expected) {
            let _ = std::fs::remove_file(&file);
            return Err(format!("the download of {} does not match its published checksum", asset.name));
        }
        prepare(install, &file, &dir, &version, &expected)
    }

    fn download(
        agent: &ureq::Agent,
        asset: &Asset,
        file: &Path,
        ctx: &egui::Context,
        status: &Arc<Mutex<Status>>,
        version: &str,
    ) -> Result<(), String> {
        if !is_release_download(&asset.url) {
            return Err(format!("{} is not one of NoteCraft's release files", asset.url));
        }
        let mut response = agent.get(&asset.url).call().map_err(|e| e.to_string())?;
        let total = asset.size.or_else(|| response.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()));
        let part = file.with_extension("part");
        // `create_new`: never write through a file or link that is already there.
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(&part).map_err(|e| e.to_string())?;
        let mut reader = response.body_mut().as_reader();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        let mut shown = 0u64;
        set(status, ctx, Status::Downloading { version: version.into(), done, total });
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if done - shown >= 1 << 20 {
                shown = done;
                set(status, ctx, Status::Downloading { version: version.into(), done, total });
            }
        }
        out.flush().map_err(|e| e.to_string())?;
        drop(out);
        if let Some(size) = asset.size
            && size != done
        {
            let _ = std::fs::remove_file(&part);
            return Err(format!("the download of {} stopped after {done} of {size} bytes", asset.name));
        }
        std::fs::rename(&part, file).map_err(|e| e.to_string())
    }

    /// Puts the verified file where it goes, or starts the script that will once the window
    /// has closed.
    fn prepare(install: Install, file: &Path, dir: &Path, version: &str, expected: &str) -> Result<Status, String> {
        let pid = std::process::id();
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let version = version.to_owned();
        match install {
            Install::WindowsMsi => {
                let script = dir.join("install.cmd");
                std::fs::write(&script, msi_script(pid, file, &exe)).map_err(|e| e.to_string())?;
                run_cmd(&script)?;
                Ok(Status::Restart { version })
            }
            Install::WindowsPortable => {
                let unpacked = dir.join("unpacked");
                let _ = std::fs::remove_dir_all(&unpacked);
                unzip(file, &unpacked)?;
                let folder = exe.parent().ok_or("NoteCraft's folder is unknown")?;
                let script = dir.join("install.cmd");
                std::fs::write(&script, portable_script(pid, &unpacked_root(&unpacked), folder, &exe)).map_err(|e| e.to_string())?;
                run_cmd(&script)?;
                Ok(Status::Restart { version })
            }
            Install::LinuxAppImage => {
                let target = std::env::var_os("APPIMAGE").map(PathBuf::from).ok_or("this AppImage's path is unknown")?;
                // Next to the old one, so putting it in place is a rename on the same disk.
                let new = target.with_extension("AppImage.new");
                let _ = std::fs::remove_file(&new);
                std::fs::copy(file, &new).map_err(|e| format!("cannot write next to {}: {e}", target.display()))?;
                // The copy is what runs, so the copy is what is checked.
                if !matches_digest(&new, expected) {
                    let _ = std::fs::remove_file(&new);
                    return Err("the copied AppImage does not match its published checksum".into());
                }
                let script = dir.join("install.sh");
                std::fs::write(&script, appimage_script(pid, &new, &target)).map_err(|e| e.to_string())?;
                run_sh(&script)?;
                Ok(Status::Restart { version })
            }
            Install::MacApp => {
                let bundle = bundle_of(&exe).ok_or("NoteCraft's app bundle is unknown")?;
                let script = dir.join("install.sh");
                std::fs::write(&script, mac_script(pid, file, &bundle)).map_err(|e| e.to_string())?;
                run_sh(&script)?;
                Ok(Status::Restart { version })
            }
            Install::LinuxDeb | Install::LinuxRpm => {
                std::process::Command::new("xdg-open").arg(file).spawn().map_err(|e| e.to_string())?;
                Ok(Status::Handed { version, file: file.to_path_buf() })
            }
            Install::Other => Ok(Status::Manual { version }),
        }
    }

    fn run_sh(script: &Path) -> Result<(), String> {
        std::process::Command::new("/bin/sh")
            .arg(script)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn run_cmd(script: &Path) -> Result<(), String> {
        let mut command = std::process::Command::new("cmd");
        command.arg("/C").arg(script);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the batch file waits in the background, not in a console window.
            command.creation_flags(0x0800_0000);
        }
        command.spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub use net::start;

/// Builds without a downloader (the web, Android) cannot install an update themselves.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn start(ctx: egui::Context, status: std::sync::Arc<std::sync::Mutex<Status>>) {
    if let Ok(mut s) = status.lock() {
        *s = Status::Manual { version: String::new() };
    }
    ctx.open_url(egui::OpenUrl::new_tab(crate::backstage::LATEST_RELEASE));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub's answer for v0.0.8, cut down to the fields used. Real data, not made up.
    const RELEASE: &str = include_str!("../tests/fixtures/release-latest.json");
    const SUMS: &str = "3db9d6248d84d08a57e76052944215c1c01c66a1bc47f4b5ec327ddbb8c6b825  notecraft-0.0.8-windows-x64.msi\n\
                        a2c7d71e9b452c172ca6db899368118619bfead70f95b502dccc7785272f9a2c  notecraft-0.0.8-windows-x64-portable.zip\n";

    fn release() -> Release {
        parse_release(&serde_json::from_str(RELEASE).expect("fixture parses")).expect("a release")
    }

    #[test]
    fn a_release_answer_gives_its_version_and_files() {
        let r = release();
        assert_eq!(r.version, semver::Version::new(0, 0, 8));
        assert_eq!(r.page, "https://github.com/AlienTechSoftware/notecraft/releases/tag/v0.0.8");
        assert!(r.assets.iter().any(|a| a.name == "SHA256SUMS.txt"));
        let msi = r.assets.iter().find(|a| a.name.ends_with(".msi")).expect("msi");
        assert!(msi.url.starts_with("https://github.com/AlienTechSoftware/notecraft/releases/download/v0.0.8/"));
        assert_eq!(msi.size, Some(71_909_664));
        assert_eq!(
            msi.sha256.as_deref(),
            Some("3db9d6248d84d08a57e76052944215c1c01c66a1bc47f4b5ec327ddbb8c6b825"),
            "the digest GitHub gives with the file"
        );
    }

    #[test]
    fn only_notecraft_release_files_are_downloaded() {
        assert!(is_release_download("https://github.com/AlienTechSoftware/notecraft/releases/download/v0.0.8/SHA256SUMS.txt"));
        assert!(!is_release_download("https://github.com/someone-else/notecraft/releases/download/v0.0.8/x.msi"));
        assert!(!is_release_download("https://example.com/AlienTechSoftware/notecraft/releases/download/v1/x.msi"));
        assert!(!is_release_download("http://github.com/AlienTechSoftware/notecraft/releases/download/v1/x.msi"));
        assert!(!is_release_download("https://github.com/AlienTechSoftware/notecraft/releases/download/../../x"));
        assert!(!is_release_download("https://github.com/AlienTechSoftware/notecraft/releases/download/"));
        // A file in the answer that is not one of NoteCraft's downloads is left out of the release.
        let mut json: serde_json::Value = serde_json::from_str(RELEASE).expect("fixture parses");
        json["assets"][0]["browser_download_url"] = "https://example.com/evil.AppImage".into();
        let r = parse_release(&json).expect("a release");
        assert_eq!(r.assets.len(), release().assets.len() - 1);
        assert!(r.assets.iter().all(|a| is_release_download(&a.url)));
    }

    #[test]
    fn downloads_go_to_a_folder_of_this_users_own() {
        let temp = Path::new("/tmp");
        assert_eq!(download_root("linux", temp, Some("/home/a/.c".into()), Some("/home/a".into())), PathBuf::from("/home/a/.c/notecraft-update"));
        assert_eq!(download_root("linux", temp, None, Some("/home/a".into())), PathBuf::from("/home/a/.cache/notecraft-update"));
        assert_eq!(download_root("linux", temp, Some("relative".into()), None), PathBuf::from("/tmp/notecraft-update"));
        assert_eq!(
            download_root("windows", Path::new(r"C:\Users\a\AppData\Local\Temp"), None, None),
            Path::new(r"C:\Users\a\AppData\Local\Temp").join("notecraft-update")
        );
    }

    #[test]
    fn only_a_higher_version_is_newer() {
        let v = semver::Version::new(0, 0, 12);
        assert!(is_newer(&v, "0.0.11"));
        assert!(!is_newer(&v, "0.0.12"));
        assert!(!is_newer(&v, "0.0.13"));
        assert!(is_newer(&v, "not a version"));
        assert_eq!(parse_version("v1.2.3"), Some(semver::Version::new(1, 2, 3)));
    }

    #[test]
    fn each_install_gets_its_own_file() {
        let r = release();
        let name = |i: Install, arch: &str| asset_for(i, arch, &r).map(|a| a.name.clone());
        assert_eq!(name(Install::WindowsMsi, "x86_64").as_deref(), Some("notecraft-0.0.8-windows-x64.msi"));
        assert_eq!(name(Install::WindowsPortable, "x86_64").as_deref(), Some("notecraft-0.0.8-windows-x64-portable.zip"));
        assert_eq!(name(Install::LinuxAppImage, "x86_64").as_deref(), Some("notecraft-0.0.8-linux-x86_64.AppImage"));
        assert_eq!(name(Install::LinuxDeb, "x86_64").as_deref(), Some("notecraft-0.0.8-linux-x86_64.deb"));
        assert_eq!(name(Install::LinuxRpm, "x86_64").as_deref(), Some("notecraft-0.0.8-linux-x86_64.rpm"));
        // v0.0.8 had no ARM Linux or macOS files: those installs are sent to the releases page.
        assert_eq!(name(Install::LinuxAppImage, "aarch64"), None);
        assert_eq!(name(Install::MacApp, "aarch64"), None);
        assert_eq!(name(Install::WindowsMsi, "aarch64"), None);
        assert_eq!(name(Install::Other, "x86_64"), None);
    }

    #[test]
    fn the_install_kind_follows_where_notecraft_runs_from() {
        let windows = |exe: &str| Machine { os: "windows", exe: exe.into(), program_files: vec![r"C:\Program Files".into()], ..Machine::default() };
        assert_eq!(Install::of(&windows(r"C:\Program Files\NoteCraft\notecraft.exe")), Install::WindowsMsi);
        assert_eq!(Install::of(&windows(r"c:\program files\NoteCraft\notecraft.exe")), Install::WindowsMsi);
        assert_eq!(Install::of(&windows(r"C:\Program Files Extra\notecraft.exe")), Install::WindowsPortable);
        assert_eq!(Install::of(&windows(r"D:\Tools\notecraft\notecraft.exe")), Install::WindowsPortable);
        let linux = |exe: &str, appimage: Option<&str>, debian: bool, redhat: bool| Machine {
            os: "linux",
            exe: exe.into(),
            appimage: appimage.map(PathBuf::from),
            debian,
            redhat,
            ..Machine::default()
        };
        assert_eq!(Install::of(&linux("/tmp/.mount_x/usr/bin/notecraft", Some("/home/a/NoteCraft.AppImage"), true, false)), Install::LinuxAppImage);
        assert_eq!(Install::of(&linux("/usr/bin/notecraft", None, true, false)), Install::LinuxDeb);
        assert_eq!(Install::of(&linux("/usr/bin/notecraft", None, false, true)), Install::LinuxRpm);
        assert_eq!(Install::of(&linux("/home/a/notecraft/notecraft", None, true, false)), Install::Other);
        let mac = Machine { os: "macos", exe: "/Applications/NoteCraft.app/Contents/MacOS/notecraft".into(), ..Machine::default() };
        assert_eq!(Install::of(&mac), Install::MacApp);
        assert_eq!(bundle_of(&mac.exe), Some(PathBuf::from("/Applications/NoteCraft.app")));
        assert_eq!(Install::of(&Machine { os: "macos", exe: "/Users/a/target/release/notecraft".into(), ..Machine::default() }), Install::Other);
    }

    #[test]
    fn the_checksum_list_gives_each_files_digest() {
        assert_eq!(
            sha256_from_sums(SUMS, "notecraft-0.0.8-windows-x64.msi").as_deref(),
            Some("3db9d6248d84d08a57e76052944215c1c01c66a1bc47f4b5ec327ddbb8c6b825")
        );
        assert_eq!(sha256_from_sums(SUMS, "notecraft-0.0.8-windows-x64"), None);
        assert_eq!(sha256_from_sums("abc *f.zip\n", "f.zip"), None, "not a digest");
        let starred = "A2C7D71E9B452C172CA6DB899368118619BFEAD70F95B502DCCC7785272F9A2C *f.zip";
        assert_eq!(sha256_from_sums(starred, "f.zip").as_deref(), Some("a2c7d71e9b452c172ca6db899368118619bfead70f95b502dccc7785272f9a2c"));
    }

    #[test]
    fn a_files_digest_is_its_sha256() {
        let dir = notecraft_store::testing::TempDir::new("update-sha256");
        let file = dir.path().join("abc.txt");
        std::fs::write(&file, b"abc").expect("write");
        assert_eq!(sha256_file(&file).expect("hash"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn a_zip_unpacks_inside_its_folder_only() {
        use std::io::Write;
        let dir = notecraft_store::testing::TempDir::new("update-unzip");
        let zip_path = dir.path().join("portable.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).expect("zip"));
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("notecraft-1.0.0-windows-x64-portable/notecraft.exe", opts).expect("entry");
            w.write_all(b"new exe").expect("write");
            w.start_file("notecraft-1.0.0-windows-x64-portable/demo/a.txt", opts).expect("entry");
            w.write_all(b"a").expect("write");
            w.start_file("../escaped.txt", opts).expect("entry");
            w.write_all(b"no").expect("write");
            w.finish().expect("finish");
        }
        let out = dir.path().join("out");
        unzip(&zip_path, &out).expect("unzip");
        let root = unpacked_root(&out);
        assert_eq!(root, out.join("notecraft-1.0.0-windows-x64-portable"));
        assert_eq!(std::fs::read(root.join("notecraft.exe")).expect("exe"), b"new exe");
        assert!(root.join("demo/a.txt").is_file());
        assert!(!dir.path().join("escaped.txt").exists(), "an entry naming a parent folder is skipped");
    }

    #[test]
    fn paths_in_scripts_keep_spaces_quotes_and_percent_signs() {
        assert_eq!(sh_quote(Path::new("/home/a b/it's.AppImage")), r"'/home/a b/it'\''s.AppImage'");
        assert_eq!(cmd_quote(Path::new(r"C:\Users\100% me\x.msi")), r#""C:\Users\100%% me\x.msi""#);
        let msi = msi_script(42, Path::new(r"C:\T\n.msi"), Path::new(r"C:\Program Files\NoteCraft\notecraft.exe"));
        assert!(msi.contains("PID eq 42"), "{msi}");
        assert!(msi.contains(r#"msiexec /i "C:\T\n.msi" /passive /norestart"#), "{msi}");
        assert!(msi.ends_with("start \"\" \"C:\\Program Files\\NoteCraft\\notecraft.exe\"\r\n"), "{msi}");
        let portable = portable_script(7, Path::new(r"C:\T\u\p"), Path::new(r"D:\nc"), Path::new(r"D:\nc\notecraft.exe"));
        assert!(portable.contains(r#"robocopy "C:\T\u\p" "D:\nc" /E"#), "{portable}");
        let mac = mac_script(9, Path::new("/tmp/N.dmg"), Path::new("/Applications/NoteCraft.app"));
        assert!(mac.contains("kill -0 9"), "{mac}");
        assert!(mac.contains("ditto \"$src\" '/Applications/NoteCraft.app.new'"), "{mac}");
    }

    /// The AppImage script really waits for the old process, swaps the file and starts the new
    /// one: run with real files and a real shell.
    #[cfg(unix)]
    #[test]
    fn the_appimage_script_waits_then_swaps_and_starts_the_new_file() {
        let dir = notecraft_store::testing::TempDir::new("update-appimage");
        let target = dir.path().join("NoteCraft one.AppImage");
        let marker = dir.path().join("started");
        std::fs::write(&target, "old").expect("old");
        let new = dir.path().join("NoteCraft one.AppImage.new");
        std::fs::write(&new, format!("#!/bin/sh\necho new > {}\n", sh_quote(&marker))).expect("new");
        // A process that is still running for a moment stands for the window being closed.
        // It is reaped on a thread of its own, or it would linger as a zombie that `kill -0`
        // still finds.
        let mut waiting = std::process::Command::new("sleep").arg("1").spawn().expect("sleep");
        let pid = waiting.id();
        let reaper = std::thread::spawn(move || waiting.wait());
        let script = dir.path().join("install.sh");
        std::fs::write(&script, appimage_script(pid, &new, &target)).expect("script");
        let started = std::time::Instant::now();
        let status = std::process::Command::new("/bin/sh").arg(&script).status().expect("run");
        let _ = reaper.join();
        assert!(status.success());
        assert!(started.elapsed() >= std::time::Duration::from_millis(500), "it waited for the old process");
        assert!(!new.exists(), "the new file was moved into place");
        assert_eq!(std::fs::read_to_string(&marker).expect("the new AppImage ran").trim(), "new");
    }
}
