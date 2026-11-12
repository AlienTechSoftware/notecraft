//! Saving a page off the window's thread.
//!
//! A save writes a temporary file and renames it over the page. On a synced folder (a OneDrive
//! mount through onedriver) the rename alone takes about a second, and while the window waited
//! for it nothing moved: a click or a letter typed then was held up. The autosave, which fires a
//! moment after every pause in writing, hands the page here instead and the window carries on.
//! Saves that must be on disk before something else happens (opening another page, closing the
//! window, a command that reads or writes the page) wait for it with [`Saver::wait`].

use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// One save that has finished: the page, the text written, and the time the page records for it
/// or why it could not be written. `jobs` is how many saves asked for it covers: saves of one
/// page asked for while another was being written are made as one, of the newest text.
pub struct Saved {
    pub dir: PathBuf,
    pub body: String,
    pub result: Result<String, String>,
    pub jobs: usize,
}

type Writer = Box<dyn Fn(&Path, &str) -> Result<String, String> + Send>;

pub struct Saver {
    jobs: Option<mpsc::Sender<(PathBuf, String)>>,
    done: mpsc::Receiver<Saved>,
    in_flight: usize,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Default for Saver {
    fn default() -> Self {
        Self::with_writer(|dir, body| notecraft_store::page::save_body(dir, body).map_err(|e| e.to_string()))
    }
}

impl Saver {
    /// A saver that writes with `write`; the window's own uses the page store.
    pub fn with_writer(write: impl Fn(&Path, &str) -> Result<String, String> + Send + 'static) -> Self {
        let write: Writer = Box::new(write);
        let (jobs, inbox) = mpsc::channel::<(PathBuf, String)>();
        let (outbox, done) = mpsc::channel::<Saved>();
        let worker = std::thread::Builder::new()
            .name("notecraft-saver".into())
            .spawn(move || {
                while let Ok(first) = inbox.recv() {
                    // Everything asked for meanwhile: of each page, only the newest text is written.
                    let mut queue = vec![first];
                    queue.extend(inbox.try_iter());
                    let mut latest: Vec<(PathBuf, String, usize)> = Vec::new();
                    for (dir, body) in queue {
                        match latest.iter_mut().find(|(d, ..)| *d == dir) {
                            Some(entry) => {
                                entry.1 = body;
                                entry.2 += 1;
                            }
                            None => latest.push((dir, body, 1)),
                        }
                    }
                    for (dir, body, jobs) in latest {
                        let result = write(&dir, &body);
                        if outbox.send(Saved { dir, body, result, jobs }).is_err() {
                            return;
                        }
                    }
                }
            })
            .ok();
        Self { jobs: Some(jobs), done, in_flight: 0, worker }
    }

    /// Asks for `body` to be written as the page in `dir`, and returns at once.
    pub fn submit(&mut self, dir: &Path, body: &str) {
        if let Some(jobs) = &self.jobs
            && jobs.send((dir.to_path_buf(), body.to_string())).is_ok()
        {
            self.in_flight += 1;
        }
    }

    /// Whether a save asked for has not finished yet.
    pub fn busy(&self) -> bool {
        self.in_flight > 0
    }

    /// The saves that have finished since this was last asked, without waiting.
    pub fn finished(&mut self) -> Vec<Saved> {
        let out: Vec<Saved> = self.done.try_iter().collect();
        self.in_flight = self.in_flight.saturating_sub(out.iter().map(|s| s.jobs).sum());
        out
    }

    /// Waits until every save asked for is written, and gives the ones that finished.
    pub fn wait(&mut self) -> Vec<Saved> {
        let mut out = self.finished();
        while self.in_flight > 0 {
            match self.done.recv() {
                Ok(saved) => {
                    self.in_flight = self.in_flight.saturating_sub(saved.jobs);
                    out.push(saved);
                }
                Err(_) => {
                    self.in_flight = 0;
                }
            }
        }
        out
    }
}

impl Drop for Saver {
    /// Whatever was asked for is written before the window goes.
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// A writer as slow as a rename on a OneDrive mount, that keeps what it wrote.
    fn slow(wrote: Arc<Mutex<Vec<String>>>) -> impl Fn(&Path, &str) -> Result<String, String> + Send + 'static {
        move |_dir, body| {
            std::thread::sleep(Duration::from_millis(300));
            wrote.lock().expect("lock").push(body.to_string());
            Ok("now".into())
        }
    }

    /// Asking for a save returns at once, however slow the writing; waiting gives it once written.
    #[test]
    fn a_save_does_not_hold_up_the_window() {
        let wrote = Arc::new(Mutex::new(Vec::new()));
        let mut saver = Saver::with_writer(slow(wrote.clone()));
        let asked = Instant::now();
        saver.submit(Path::new("page"), "words");
        assert!(asked.elapsed() < Duration::from_millis(100), "asking took {:?}", asked.elapsed());
        assert!(saver.busy(), "the save is under way");
        let done = saver.wait();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].result, Ok("now".to_string()));
        assert_eq!(*wrote.lock().expect("lock"), vec!["words".to_string()]);
        assert!(!saver.busy(), "and nothing is left to write");
    }

    /// Saves of one page asked for while it is being written become one, of the newest text.
    ///
    /// The first write says when it has begun and then holds until the later saves have been
    /// asked for, so the order is certain. A fixed sleep did not promise that the worker had
    /// picked up the first save yet; when it had not, all three were rightly made as one and
    /// the test failed (issue #116).
    #[test]
    fn saves_asked_for_meanwhile_write_only_the_newest_text() {
        let wrote = Arc::new(Mutex::new(Vec::new()));
        let (began, writing) = std::sync::mpsc::channel::<()>();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let gate = Mutex::new(Some((began, held)));
        let slow = slow(wrote.clone());
        let mut saver = Saver::with_writer(move |dir, body| {
            if let Some((began, held)) = gate.lock().expect("lock").take() {
                let _ = began.send(());
                let _ = held.recv_timeout(Duration::from_secs(10));
            }
            slow(dir, body)
        });
        saver.submit(Path::new("page"), "one");
        writing.recv_timeout(Duration::from_secs(10)).expect("the first save began");
        saver.submit(Path::new("page"), "two");
        saver.submit(Path::new("page"), "three");
        release.send(()).expect("let the first save finish");
        let done = saver.wait();
        let wrote = wrote.lock().expect("lock").clone();
        assert_eq!(wrote, vec!["one".to_string(), "three".to_string()], "the newest text is written last, and only once");
        assert_eq!(done.iter().map(|saved| saved.jobs).collect::<Vec<_>>(), vec![1, 2], "the second write answers for both saves");
        assert!(!saver.busy(), "and nothing is left to write");
    }

    /// A save that fails says why.
    #[test]
    fn a_failed_save_says_why() {
        let mut saver = Saver::with_writer(|_, _| Err("the folder is gone".into()));
        saver.submit(Path::new("page"), "words");
        let done = saver.wait();
        assert_eq!(done[0].result, Err("the folder is gone".to_string()));
    }
}
