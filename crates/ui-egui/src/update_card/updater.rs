//! The update flow of an egui app in one place: a check at start or on request, the startup prompt,
//! and the self-update worker. An app creates one [`Updater`], calls [`Updater::tick`] and
//! [`Updater::show`] each frame, and acts on the [`Action`] it returns, so every app gets the same
//! handling of failures, dropped workers, repaints and quitting.
//!
//! An app the launcher keeps on another version (see [`craft_update::pins`]) offers nothing itself: the
//! check reads the shared choice first, and a requested check reports [`Status::Managed`] while
//! [`Updater::managed`] says which choice it is.
//!
//! An update ends with the app closing. On Windows the update installs only after the app has
//! exited, so the updater says it is ready rather than installed, and the first start afterwards
//! reports what came of it: [`Status::Updated`] or [`Status::InstallFailed`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::dialog::{StartupPrompt, StartupPromptResponse};
use craft_update::pins::{Choice, Pins};
use craft_update::release::Release;
use craft_update::self_update::{Outcome, Restart};
use craft_update::translate::{Note, Translate, english};
use craft_update::{App, Http, Paths, Platform, Stop, check_for_update, self_update};

/// Asks for the newest release (blocking; it runs on its own thread). Tests stand in for the network
/// with one; without it the check asks GitHub.
pub type CheckSource = Arc<dyn Fn() -> Result<Option<Release>, String> + Send + Sync>;

/// What a requested check came to, for a window that answers Help ▸ Check for Updates, and what
/// the last update came to, at the first start after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing to report.
    Idle,
    /// A requested check is running.
    Checking,
    /// A requested check found nothing newer.
    UpToDate,
    /// A requested check failed.
    Failed(String),
    /// A requested check found an update downloading and installing; the startup prompt shows
    /// its progress.
    Installing,
    /// A requested check found an update ready and waiting for the app to close
    /// ([`READY_TEXT`]). [`Updater::restart_to_update`] asks the app to close.
    Ready,
    /// A requested check found that the launcher keeps this app on a version of its choosing
    /// ([`Updater::managed`]), so it does not update itself.
    Managed,
    /// This start follows an update that installed this version.
    Updated(String),
    /// The last update could not be installed, for this reason.
    InstallFailed(String),
}

/// What [`Status::Ready`] says: on Windows the update installs after the app exits; elsewhere it
/// is already in place and takes effect when the app restarts.
pub const READY_TEXT: &str =
    if cfg!(windows) { "Update ready. Close {app} to install it." } else { "The update is installed. Close {app} to finish; it restarts by itself." };

/// What the app should do after [`Updater::show`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Open this page in the browser.
    OpenUrl(String),
    /// Close the app through its normal close path (which asks about unsaved work): an update is
    /// ready and a helper installs it and restarts the app once the app has exited. This is a
    /// request: the user may cancel the close, and the update then stays ready, with
    /// [`Status::Ready`] for a requested check and Restart to Update in the prompt. The helper
    /// starts only at the exit: when the app drops the [`Updater`] as it exits, or
    /// calls [`Updater::start_pending_update`] in `on_exit`, which an app that ends the process
    /// there without dropping the `Updater` must do. An installation still in progress when
    /// such an app ends the process is lost, since its worker thread ends with the process.
    Close,
}

type Checked = Result<Option<Release>, String>;

/// Why an installation failed: the installer's own words, shown as they are, or the updater's,
/// shown in the interface language.
#[derive(Debug)]
enum Failure {
    Installer(String),
    Updater(Note),
}

impl Failure {
    /// The worker panicked.
    fn panicked() -> Self {
        Self::Updater(Note::new("The update failed unexpectedly.", &[]))
    }

    /// The worker went away without an answer.
    fn dropped() -> Self {
        Self::Updater(Note::new("The update stopped unexpectedly.", &[]))
    }

    /// The reason in English, for the log and for the record the next start reads.
    fn reason(&self) -> String {
        match self {
            Self::Installer(e) => e.clone(),
            Self::Updater(note) => note.translated(english),
        }
    }
}

/// Logs a failed update and records it under `paths`, so that the next start reports it
/// ([`Status::InstallFailed`]). Used at the exit, where no prompt can show the reason.
fn record(app: &App, paths: Option<&Paths>, reason: &str) {
    log::warn!("{} self-update failed: {reason}", app.name);
    if let Some(paths) = paths {
        self_update::record_failure(paths, reason);
    }
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Self::Installer(e)
    }
}

impl From<&str> for Failure {
    fn from(e: &str) -> Self {
        Self::Installer(e.to_owned())
    }
}

pub struct Updater {
    app: &'static App,
    version: String,
    source: Option<CheckSource>,
    /// Whether this copy can install an update in place, decided once by the app (see
    /// [`Updater::with_installable`]). `None`: the card asks the system itself.
    installable: Option<bool>,
    status: Status,
    /// The check running (or queued behind it) was asked for by the user.
    requested: bool,
    started: bool,
    check: Option<Receiver<Checked>>,
    install: Option<(Receiver<Result<Restart, Failure>>, Release)>,
    pub(crate) prompt: Option<StartupPrompt>,
    /// A finished installation waiting for the app to exit: what still has to start then, and
    /// the release it installs.
    restart: Option<(Restart, Release)>,
    /// An update is ready and waits for the app to exit; nothing starts a second one.
    installed: bool,
    /// [`Self::show_if`] returns [`Action::Close`] at its next frame that may close: once when an
    /// installation finishes, and again for each [`Self::restart_to_update`].
    close_wanted: bool,
    /// The folders that hold the outcome records ([`self_update::take_outcome`]) in place of the
    /// app's own (tests).
    paths: Option<Paths>,
    /// Why the last update could not be installed, until a prompt that offers the update again
    /// shows it.
    failed_reason: Option<String>,
    /// A page to open on the next frame (macOS installs by hand: see `super::macos`).
    pub(crate) pending_open: Option<String>,
    /// Set when the app exits: the `Updater` is dropped or [`Updater::start_pending_update`] is
    /// called. An install worker sends its outcome while holding it, and starts a finished
    /// installation's helper itself once it is set.
    exiting: Arc<Mutex<bool>>,
    /// Whether the app may close now: no unsaved work and no page conflict. Set each frame by
    /// [`Self::show_if`]; the install worker waits for it before it replaces the installed copy.
    may_exit: Arc<AtomicBool>,
    /// The version-choice file to read in place of [`Pins::default_path`] (tests).
    pins_path: Option<PathBuf>,
    /// The launcher's choice for this app when it is not stable, as of the last check or offer.
    managed: Option<Choice>,
    /// Puts the prompt and its messages in the interface language ([`craft_update::translate`]).
    translate: Translate,
}

impl Updater {
    pub fn new(app: &'static App, version: impl Into<String>) -> Self {
        let version = version.into();
        let version = self_update::current_version(app, &version).map_or(version, |v| v.to_string());
        Self {
            app,
            version,
            source: None,
            installable: None,
            status: Status::Idle,
            requested: false,
            started: false,
            check: None,
            install: None,
            prompt: None,
            restart: None,
            installed: false,
            close_wanted: false,
            paths: None,
            failed_reason: None,
            pending_open: None,
            exiting: Arc::new(Mutex::new(false)),
            may_exit: Arc::new(AtomicBool::new(false)),
            pins_path: None,
            managed: None,
            translate: english,
        }
    }

    /// Words the startup prompt and the messages it shows with `translate`, in the app's
    /// interface language ([`craft_update::translate`]). Errors from the network stay as they came.
    #[must_use]
    pub fn with_translator(mut self, translate: Translate) -> Self {
        self.translate = translate;
        // A prompt already open (an offer made before) speaks the new language from its next draw.
        if let Some(prompt) = &mut self.prompt {
            prompt.translate = translate;
        }
        self
    }

    /// The startup prompt for `release`, in the interface language.
    fn prompt_for(&self, release: Release) -> StartupPrompt {
        let mut prompt = StartupPrompt::new(self.app, release, self.version.clone()).with_translator(self.translate);
        // The card's button follows the decision the app made once, not a fresh check while drawing.
        if let Some(installable) = self.installable {
            prompt.is_self_updating = installable;
        }
        prompt
    }

    /// Opens the prompt offering `release`. A failed update offered again says why the last
    /// attempt failed, once.
    fn open_offer(&mut self, release: Release) {
        let mut prompt = self.prompt_for(release);
        if let Some(reason) = self.failed_reason.take() {
            prompt.set_note(Note::new("The update could not be installed: {reason}", &[("reason", &reason)]));
        }
        self.prompt = Some(prompt);
    }

    /// [`READY_TEXT`], translated when the prompt draws.
    fn installed_note(&self) -> Note {
        Note::new(READY_TEXT, &[("app", self.app.name)])
    }

    /// Decides once whether this copy can install an update in place. The card's button uses this
    /// answer, so the button cannot change between one frame and the next.
    pub fn with_installable(mut self, installable: bool) -> Self {
        self.installable = Some(installable);
        self
    }

    /// Replaces the network with `source` (tests).
    pub fn with_source(mut self, source: CheckSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Reads and writes the update's outcome records under `paths` instead of the app's own
    /// folders (tests).
    pub fn with_paths(mut self, paths: Paths) -> Self {
        self.paths = Some(paths);
        self
    }

    /// The folders of the outcome records: [`Self::with_paths`]'s, else the app's own. Unit tests
    /// never read or write the app's own records.
    fn outcome_paths(&self) -> Option<Paths> {
        self.paths.clone().or_else(|| if cfg!(test) { None } else { Paths::for_app(self.app).ok() })
    }

    /// What the last update came to, once: [`Status::Updated`] when it installed the version
    /// running now, [`Status::InstallFailed`] with the reason when it failed.
    fn read_outcome(&mut self) {
        let Some(paths) = self.outcome_paths() else { return };
        match self_update::take_outcome(&paths, &self.version) {
            Some(Outcome::Installed(version)) => self.status = Status::Updated(version.to_string()),
            Some(Outcome::Failed(reason)) => {
                log::warn!("{} self-update failed: {reason}", self.app.name);
                self.failed_reason = Some(reason.clone());
                self.status = Status::InstallFailed(reason);
            }
            None => {}
        }
    }

    /// Reads the version choices from `path` instead of the launcher's file (tests).
    pub fn with_pins_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.pins_path = Some(path.into());
        self
    }

    /// The launcher's choice for this app when it keeps it off stable releases (a pinned version,
    /// nightlies, or a choice this version cannot read), as of the last check. A window answering
    /// Help ▸ Check for Updates can say so instead of "newest version".
    pub fn managed(&self) -> Option<&Choice> {
        self.managed.as_ref()
    }

    /// Reads the launcher's choice for this app; remembers it when it is not stable.
    fn read_choice(&mut self) -> bool {
        let pins = match &self.pins_path {
            Some(path) => Pins::load(path),
            None => Pins::load_default(),
        };
        self.managed = Some(pins.choice(self.app.id)).filter(|c| *c != Choice::Stable);
        if let Some(choice) = &self.managed {
            log::info!("{} follows the launcher's choice ({choice:?}); no update offered here", self.app.name);
            // A prompt opened before the pin would still install the stable release. One that is
            // installing or finished stays: `tick` reports through it.
            if self.install.is_none() && !self.installed {
                self.prompt = None;
            }
        }
        self.managed.is_some()
    }

    /// The check at start, once. Later calls do nothing. It first reads what the last update came
    /// to, once: [`Self::status`] is then [`Status::Updated`] or [`Status::InstallFailed`], and
    /// the app shows its [`CheckResultDialog`](crate::dialog::CheckResultDialog) while the status
    /// is not [`Status::Idle`]. The check itself stays quiet unless it finds a release.
    pub fn check_at_start(&mut self, ctx: &egui::Context) {
        if !std::mem::replace(&mut self.started, true) {
            self.read_outcome();
            self.begin(ctx, false);
        }
    }

    /// Help ▸ Check for Updates: a check whose outcome [`Self::status`] reports. A check already
    /// running becomes a requested one. While an update installs or waits for the app to close,
    /// nothing starts and the status says so ([`Status::Installing`], [`Status::Ready`]).
    pub fn check_now(&mut self, ctx: &egui::Context) {
        self.started = true;
        self.begin(ctx, true);
    }

    /// Asks the app to close so that a ready update installs: the next [`Self::show_if`] that may
    /// close returns [`Action::Close`]. Restart to Update calls it after the user canceled an
    /// earlier close. No close is asked for while no update is ready.
    pub fn restart_to_update(&mut self) {
        self.close_wanted = true;
    }

    /// Opens the startup prompt for a release the app found itself. After a failed update the
    /// prompt says why it failed, like the one a check opens. Opening the prompt sets
    /// [`Self::status`] to [`Status::Idle`].
    pub fn offer(&mut self, release: Release) {
        if self.install.is_none() && !self.installed && !self.read_choice() {
            self.open_offer(release);
            self.status = Status::Idle;
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Dismisses the outcome of a requested check. One still running carries on, and reports
    /// like the check at start (a prompt if there is a release, otherwise nothing).
    pub fn dismiss_status(&mut self) {
        self.requested = false;
        self.status = Status::Idle;
    }

    /// Whether the startup prompt is showing.
    pub fn prompt_open(&self) -> bool {
        self.prompt.is_some()
    }

    fn begin(&mut self, ctx: &egui::Context, requested: bool) {
        if self.install.is_some() || self.installed {
            if requested {
                self.status = self.pending_status();
            }
            return;
        }
        if requested {
            self.requested = true;
            self.status = Status::Checking;
        }
        if self.check.is_some() {
            return;
        }
        // The file is a few bytes, read once per check; the launcher may have changed it since.
        if self.read_choice() {
            self.finish_check(Ok(None));
            return;
        }
        let (tx, rx) = channel();
        let (source, app, version, ctx) = (self.source.clone(), self.app, self.version.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
            let ask = || match source {
                Some(ask) => ask(),
                None => Http::new(&format!("{}/Update", app.name)).and_then(|http| check_for_update(app, &http, &version)),
            };
            let answer =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(ask)).unwrap_or_else(|_| Err("the update check failed unexpectedly".into()));
            let _ = tx.send(answer);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.check = Some(rx),
            Err(e) => self.finish_check(Err(format!("couldn't start the update check ({e})"))),
        }
    }

    /// The answer to a requested check while an update installs or waits for the app to exit. A
    /// helper that already started while the app kept running may have failed: its recorded
    /// reason is the answer then, the update is on offer again, and an open prompt says why
    /// instead of offering Restart to Update.
    fn pending_status(&mut self) -> Status {
        if self.install.is_some() {
            return Status::Installing;
        }
        if self.restart.is_none()
            && let Some(reason) = self.outcome_paths().and_then(|paths| self_update::take_failure(&paths))
        {
            log::warn!("{} self-update failed: {reason}", self.app.name);
            let reason = reason.trim().to_owned();
            self.installed = false;
            self.close_wanted = false;
            match &mut self.prompt {
                Some(prompt) => {
                    prompt.is_installing = false;
                    prompt.is_ready = false;
                    prompt.set_note(Note::new("The update could not be installed: {reason}", &[("reason", &reason)]));
                }
                // The next prompt that offers the update says why.
                None => self.failed_reason = Some(reason.clone()),
            }
            return Status::InstallFailed(reason);
        }
        Status::Ready
    }

    /// Sets the outcome of a check. The check at start leaves the status as it is: nothing, or
    /// the outcome of the last update ([`Self::read_outcome`]).
    fn finish_check(&mut self, answer: Checked) {
        let requested = std::mem::take(&mut self.requested);
        // A check that was running when an installation started answers with the installation's
        // state: the update installing, ready, or the recorded helper failure.
        if self.install.is_some() || self.installed {
            match &answer {
                Ok(Some(release)) => log::info!("{} update check found {} while an update installs", self.app.name, release.tag),
                Ok(None) => log::info!("{} update check found nothing newer while an update installs", self.app.name),
                Err(e) => log::warn!("{} update check failed while an update installs: {e}", self.app.name),
            }
            if requested {
                self.status = self.pending_status();
            }
            return;
        }
        // The launcher may have pinned or unpinned the app while the check ran: whatever the
        // answer, an app it manages offers nothing and says so.
        if self.read_choice() {
            if let Err(e) = &answer {
                log::info!("{} update check failed ({e}), but the launcher manages its version", self.app.name);
            }
            if requested {
                self.status = Status::Managed;
            }
            return;
        }
        // Only a release newer than this copy is offered. The same or an older one installs nothing,
        // and must not be recorded as an update.
        let answer = match answer {
            Ok(Some(release))
                if !release.version.as_ref().is_some_and(|v| {
                    craft_update::version::parse(&self.version).is_some_and(|installed| craft_update::version::is_newer(v, &installed))
                }) =>
            {
                log::info!("update: {} is not newer than {}; nothing offered", release.tag, self.version);
                Ok(None)
            }
            other => other,
        };
        match answer {
            Ok(Some(release)) => {
                match &mut self.prompt {
                    None => self.open_offer(release),
                    // A prompt the app opened before (`offer`) says why the last update failed.
                    Some(prompt) => {
                        if let Some(reason) = self.failed_reason.take() {
                            prompt.set_note(Note::new("The update could not be installed: {reason}", &[("reason", &reason)]));
                        }
                    }
                }
                self.status = Status::Idle;
            }
            Ok(None) if requested => self.status = Status::UpToDate,
            Ok(None) => {}
            Err(e) => {
                log::warn!("{} update check failed: {e}", self.app.name);
                if requested {
                    self.status = Status::Failed(e);
                }
            }
        }
    }

    /// Picks up a finished check or installation. Call once per frame, before [`Self::show`]. It
    /// keeps the UI awake while either is running.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.check {
            match rx.try_recv() {
                Ok(answer) => {
                    self.check = None;
                    self.finish_check(answer);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.check = None;
                    self.finish_check(Err("the update check stopped unexpectedly".into()));
                }
            }
        }
        if let Some((rx, _)) = &self.install {
            let outcome = match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    None
                }
                Err(TryRecvError::Disconnected) => Some(Err(Failure::dropped())),
            };
            if let Some(outcome) = outcome {
                let release = self.install.take().map(|(_, release)| release);
                match outcome {
                    Ok(restart) => {
                        self.installed = true;
                        self.restart = release.map(|release| (restart, release));
                        self.close_wanted = true;
                        let note = self.installed_note();
                        if let Some(prompt) = &mut self.prompt {
                            prompt.is_installing = false;
                            prompt.is_ready = true;
                            prompt.set_note(note);
                        }
                        if matches!(self.status, Status::Installing) {
                            self.status = Status::Ready;
                        }
                    }
                    Err(failure) => {
                        match &failure {
                            Failure::Installer(e) => log::warn!("update: install failed for {}: {e}", self.app.name),
                            Failure::Updater(note) => log::warn!("{} self-update failed: {}", self.app.name, note.translated(english)),
                        }
                        // The prompt says why; a result window showing the installation closes.
                        if matches!(self.status, Status::Installing) {
                            self.status = Status::Idle;
                        }
                        // The prompt may have been dismissed meanwhile: bring it back with the reason.
                        if self.prompt.is_none()
                            && let Some(release) = release
                        {
                            self.prompt = Some(self.prompt_for(release));
                        }
                        if let Some(prompt) = &mut self.prompt {
                            prompt.is_installing = false;
                            match failure {
                                Failure::Installer(e) => prompt.set_message(e),
                                Failure::Updater(note) => prompt.set_note(note),
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draws the startup prompt and starts what it asks for.
    pub fn show(&mut self, ctx: &egui::Context) -> Action {
        self.show_if(ctx, true)
    }

    /// Like [`Self::show`], but an installation only starts, and a ready update only asks the app
    /// to close, when `can_start` is true. An app passes `false` while it holds unsaved work.
    pub fn show_if(&mut self, ctx: &egui::Context, can_start: bool) -> Action {
        self.may_exit.store(can_start, Ordering::SeqCst);
        if let Some(url) = self.pending_open.take() {
            return Action::OpenUrl(url);
        }
        // The close waits for the app to hold no unsaved work: the user may have edited while the
        // update installed. It is only asked for: the helper, which waits just a minute for the
        // app to exit, starts at the exit itself (`start_pending_update`), so a close the user
        // cancels leaves the update ready.
        if can_start && self.restart.is_some() && std::mem::take(&mut self.close_wanted) {
            return Action::Close;
        }
        let Some(prompt) = &mut self.prompt else { return Action::None };
        match prompt.show(ctx) {
            StartupPromptResponse::Pending => Action::None,
            StartupPromptResponse::UpdateAndRestart => {
                self.request_install(ctx, can_start);
                Action::None
            }
            StartupPromptResponse::Download => {
                let action = download_action(self.app, &prompt.release);
                self.prompt = None;
                action
            }
            StartupPromptResponse::Later => {
                // An installation carries on and reports through `tick`.
                self.prompt = None;
                Action::None
            }
        }
    }

    /// Starts the helper of a ready update, so that the update installs once the app has exited.
    /// Call it once the app's exit is certain (a close nothing can cancel any more, in `on_exit`),
    /// whether or not the exit came through [`Action::Close`]; dropping the `Updater` calls it
    /// too. Later calls do nothing. An installation still running at the first call starts its
    /// helper itself when it finishes, as long as the process is still running then, and no
    /// longer reports through [`Updater::tick`]; an app that ends the process in `on_exit` loses
    /// an installation still in progress, since its worker thread ends with the process. A
    /// helper that cannot start, or an installation that failed and was not reported yet, is
    /// recorded, and the next start reports it ([`Status::InstallFailed`]).
    pub fn start_pending_update(&mut self) {
        // Held while the channel is read: a worker sent its outcome before, and it is read here,
        // or it sends after and starts its helper (or records its failure) itself.
        let exiting = Arc::clone(&self.exiting);
        let mut exiting = exiting.lock().unwrap_or_else(PoisonError::into_inner);
        *exiting = true;
        // An installation that finished since the last `tick`, or whose worker ended without a word.
        // One still running is let go: it now reports through `deliver` alone.
        if let Some((rx, release)) = self.install.take()
            && let Some(outcome) = match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(Failure::dropped())),
            }
        {
            match outcome {
                Ok(restart) => {
                    self.restart = Some((restart, release));
                    self.installed = true;
                }
                Err(failure) => record(self.app, self.outcome_paths().as_ref(), &failure.reason()),
            }
        }
        if let Some((restart, _)) = self.restart.take()
            && let Err(e) = restart.start()
        {
            record(self.app, self.outcome_paths().as_ref(), &e);
        }
    }

    fn request_install(&mut self, ctx: &egui::Context, can_start: bool) {
        if can_start {
            self.start_install(ctx);
        } else if let Some(prompt) = &mut self.prompt {
            prompt.set_note(Note::new("Save or close your open work first; the update restarts the app.", &[]));
        }
    }

    /// macOS installs by hand until signing exists: the card explains it and opens the releases
    /// page. Returns true when that route was taken.
    pub(crate) fn manual_on_this_os(&mut self, os_is_macos: bool) -> bool {
        if !os_is_macos {
            return false;
        }
        if let Some(prompt) = &mut self.prompt {
            prompt.set_note(Note::new(super::macos::MANUAL_UPDATE_NOTE, &[]));
        }
        self.pending_open = Some(super::macos::manual_update_page().to_string());
        log::info!("update: macOS installs by hand for now; opening the releases page");
        true
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        if self.manual_on_this_os(cfg!(target_os = "macos")) {
            return;
        }
        // The launcher may have pinned the app while the prompt was open; that closes it.
        if self.install.is_none() && !self.installed && self.read_choice() {
            return;
        }
        let installed_note = self.installed_note();
        let paths = self.outcome_paths();
        let Some(prompt) = &mut self.prompt else { return };
        if self.installed {
            // Restart to update: the app is asked to close again.
            prompt.set_note(installed_note);
            if self.restart.is_some() {
                self.close_wanted = true;
            }
            return;
        }
        let Some(installed) = self_update::current(self.app) else {
            log::warn!("update: Update and Restart pressed, but this installation cannot be updated in place");
            prompt.set_note(Note::new("Cannot auto-update this installation type. Please download manually.", &[]));
            prompt.is_installing = false;
            return;
        };
        log::info!("update: Update and Restart pressed: {} from {:?}", prompt.release.tag, installed);
        prompt.is_installing = true;
        prompt.clear_message();
        self.failed_reason = None;
        let (tx, rx) = channel();
        let (app, version, ctx, release) = (self.app, self.version.clone(), ctx.clone(), prompt.release.clone());
        let worker_release = release.clone();
        let exiting = handoff(&mut self.exiting);
        let may_exit = Arc::clone(&self.may_exit);
        let spawned = std::thread::Builder::new().name("self-update".into()).spawn(move || {
            let run = || install(app, &installed, worker_release, &version, &may_exit).map_err(Failure::Installer);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err(Failure::panicked()));
            deliver(app, &tx, &exiting, paths.as_ref(), outcome);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.install = Some((rx, release)),
            Err(e) => {
                prompt.is_installing = false;
                prompt.set_note(Note::new("Couldn't start the update ({error})", &[("error", &e.to_string())]));
            }
        }
    }
}

impl Drop for Updater {
    /// The app is exiting: an installed update that is still waiting starts its helper, which
    /// waits for this process to exit before it installs.
    fn drop(&mut self) {
        self.start_pending_update();
    }
}

/// The flag a new install worker sends its outcome under (the `Updater`'s `exiting`). After
/// [`Updater::start_pending_update`] set it while the app kept running (a helper that then failed
/// puts the update on offer again), a new flag replaces it, so that the new worker reports
/// through `tick` instead of starting its helper at once. A worker let go before keeps the old one.
fn handoff(exiting: &mut Arc<Mutex<bool>>) -> Arc<Mutex<bool>> {
    if *exiting.lock().unwrap_or_else(PoisonError::into_inner) {
        *exiting = Arc::new(Mutex::new(false));
    }
    Arc::clone(exiting)
}

/// Sends the install worker's outcome to the [`Updater`]. When the app is exiting (the `Updater`
/// was dropped or [`Updater::start_pending_update`] was called), the worker starts a finished
/// installation's helper itself, since nothing will start it at a close. A helper that cannot
/// start, or a failed installation, is then recorded under `paths` for the next start.
fn deliver(app: &App, tx: &Sender<Result<Restart, Failure>>, exiting: &Mutex<bool>, paths: Option<&Paths>, outcome: Result<Restart, Failure>) {
    let gone = exiting.lock().unwrap_or_else(PoisonError::into_inner);
    let unsent = if *gone { Some(outcome) } else { tx.send(outcome).err().map(|SendError(outcome)| outcome) };
    drop(gone);
    match unsent {
        Some(Ok(restart)) => {
            if let Err(e) = restart.start() {
                record(app, paths, &e);
            }
        }
        Some(Err(failure)) => record(app, paths, &failure.reason()),
        None => {}
    }
}

fn install(
    app: &'static App,
    installed: &self_update::Install,
    mut release: Release,
    version: &str,
    may_exit: &Arc<AtomicBool>,
) -> Result<Restart, String> {
    let http = Http::new(&format!("{}/Update", app.name)).map_err(|e| format!("could not initialize HTTP: {e}"))?;
    if release.assets.is_empty()
        && let Some(full) = check_for_update(app, &http, version).map_err(|e| format!("could not fetch release assets: {e}"))?
    {
        release = full;
    }
    let paths = Paths::for_app(app).map_err(|e| format!("could not find paths: {e}"))?;
    let platform = Platform::current().ok_or("unsupported platform for self update")?;
    // The download and its check happen now; the installed copy is replaced only once the app
    // may close. Unsaved work holds the swap back, so nothing is replaced under an open page.
    let may_exit = Arc::clone(may_exit);
    let wait_to_exit = move || {
        while !may_exit.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    };
    self_update::prepare_update(app, installed, &release, platform, &http, &paths, &Stop::default(), &mut |_| {}, &wait_to_exit, Some(version))
}

/// Download in the prompt: the release page, when it is a safe web link (an app may build its
/// `Release` from its own feed or cache, so the link isn't always one GitHub's parser checked).
/// A link that fails the check falls back to the app's releases page: the prompt has closed, so
/// Download must still open something.
fn download_action(app: &App, release: &Release) -> Action {
    let safe = craft_update::release::safe_web_link;
    if safe(&release.html_url) {
        return Action::OpenUrl(release.html_url.clone());
    }
    let releases = app.releases_url();
    if safe(&releases) { Action::OpenUrl(releases) } else { Action::None }
}
