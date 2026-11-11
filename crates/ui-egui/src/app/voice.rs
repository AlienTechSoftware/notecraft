//! Native dictation: the mic button and shortcut, the recording bar, the preview at the
//! insertion point, the setup screen, and the reviewed editor transaction.

use super::*;
use craft_voice_core::State;
use craft_voice_runtime::audio::PcmAudio;
use craft_voice_runtime::{Cancellation, Runtime, RuntimeEvent, Transcriber, WhisperConfig};
use notecraft_engine::{InvocationOrigin, VoiceEffect, VoiceUpdate};
use std::hash::{Hash, Hasher};
use std::io::Read;

/// Session ids for the setup screen's checks, kept apart from dictation sessions.
const MIC_TEST: u64 = u64::MAX;
const RECOGNIZER_TEST: u64 = u64::MAX - 1;
/// Holding the shortcut longer than this makes it push-to-talk; a shorter press toggles.
const HOLD: Duration = Duration::from_millis(600);
/// How long the microphone check listens.
const MIC_TEST_FOR: Duration = Duration::from_secs(4);
/// Where whisper.cpp's program and models come from, for the setup screen's link.
const WHISPER_HELP_URL: &str = "https://github.com/ggml-org/whisper.cpp#quick-start";
/// The languages offered by name; any other code can still be typed into settings.
const LANGUAGES: [(&str, &str); 11] = [
    ("auto", "voice.detect_automatically_25a36033"),
    ("en", "ui.english"),
    ("es", "voice.spanish_8df7f1b3"),
    ("fr", "voice.french_44389f6a"),
    ("de", "voice.german_da91388c"),
    ("pt", "voice.portuguese_23882c57"),
    ("it", "voice.italian_485b57ad"),
    ("nl", "voice.dutch_334a85f1"),
    ("zh", "voice.chinese_15dc1886"),
    ("ja", "voice.japanese_9239c22d"),
    ("ko", "voice.korean_dec66c5b"),
];

pub(super) struct VoiceUi {
    pub editor_epoch: u64,
    ticket_counter: u64,
    target: Option<InsertionTicket>,
    runtime: Option<Runtime>,
    executable: String,
    model: String,
    language: String,
    device: Option<String>,
    devices: Vec<String>,
    devices_listed: bool,
    shortcut: String,
    /// Show the words for review before they go in the page (the default), or put them in at once.
    review_first: bool,
    review: String,
    review_revision: u64,
    /// The words in the preview are open for typing.
    editing: bool,
    /// When the shortcut went down, to tell a tap (toggle) from a hold (push-to-talk).
    pressed_at: Option<Instant>,
    pending_stop: bool,
    /// An automatic insert was tried for this session, so a failure is not retried every frame.
    auto_tried: bool,
    seconds: f32,
    /// The latest input level, and the loudest since recording began (0 to 1).
    level: f32,
    loudest: f32,
    error: Option<String>,
    setup_open: bool,
    /// Why the setup screen opened by itself, shown at its top.
    setup_reason: Option<&'static str>,
    mic_test: Option<MicTest>,
    mic_result: Option<Result<String, String>>,
    recognizer_test: Option<Runtime>,
    recognizer_result: Option<Result<String, String>>,
}

struct MicTest {
    runtime: Runtime,
    started: Instant,
    level: f32,
    loudest: f32,
    heard_audio: bool,
}

impl VoiceUi {
    pub fn new(settings: &notecraft_store::Settings) -> Self {
        Self {
            editor_epoch: 0,
            ticket_counter: 0,
            target: None,
            runtime: None,
            executable: settings.get("voice_executable").unwrap_or_default(),
            model: settings.get("voice_model").unwrap_or_default(),
            language: settings.get("voice_language").unwrap_or_else(|| "auto".into()),
            device: settings.get("voice_device").filter(|v| !v.is_empty()),
            devices: Vec::new(),
            devices_listed: false,
            shortcut: settings.get("voice_shortcut").filter(|k| ["F8", "F9", "F10"].contains(&k.as_str())).unwrap_or_else(|| "F8".into()),
            review_first: settings.get("voice_review").is_none_or(|v| v != "false"),
            review: String::new(),
            review_revision: 0,
            editing: false,
            pressed_at: None,
            pending_stop: false,
            auto_tried: false,
            seconds: 0.0,
            level: 0.0,
            loudest: 0.0,
            error: None,
            setup_open: false,
            setup_reason: None,
            mic_test: None,
            mic_result: None,
            recognizer_test: None,
            recognizer_result: None,
        }
    }
}

/// Where the dictated words go: at the caret, or as new paragraphs at the end of the page when
/// there is no caret or it sits somewhere words cannot go (a table, code, a picture).
#[derive(Clone)]
enum Anchor {
    Caret(pageedit::Caret),
    End,
}

#[derive(Clone)]
pub(super) struct InsertionTicket {
    id: String,
    epoch: u64,
    generation: u64,
    dir: PathBuf,
    body: String,
    source: u64,
    anchor: Anchor,
    /// Input selection at capture time, including unsupported targets that append instead.
    caret_snapshot: Option<pageedit::Caret>,
    across_snapshot: Option<pageedit::Across>,
    /// The page's title, to name it if the person moves away before inserting.
    title: String,
}

/// What a person can do about an error, offered as a button beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fix {
    None,
    MicSettings,
    Setup,
    PageView,
}

/// An error from the recorder or recognizer, in words a person can act on.
fn explain(message: &str) -> (String, Fix) {
    let m = message.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| m.contains(w));
    if any(&[
        "sent no audio",
        "no matching input device",
        "cannot open microphone",
        "cannot list microphones",
        "cannot start microphone",
        "cannot create microphone stream",
        "microphone failed",
        "microphone disconnected",
    ]) {
        let fix = if MIC_SETTINGS { Fix::MicSettings } else { Fix::Setup };
        return (
            crate::i18n::format_message_id("voice.notecraft_can_t_hear_your_microphone_check_that_it_5f82d9f8", &[("system", system_name())]),
            fix,
        );
    }
    if m.contains("buffer overflow") {
        return (crate::tl_id!("voice.the_computer_fell_behind_while_recording_try_again_e30d7359").into(), Fix::None);
    }
    if m.contains("timed out") {
        return (crate::tl_id!("voice.recognition_took_too_long_a_smaller_model_such_as_fff73af3").into(), Fix::Setup);
    }
    if any(&["whisper-cli", "does not advertise", "recognizer exited", "whisper model", "language code"]) {
        return (crate::tl_id!("voice.the_speech_recognizer_isn_t_set_up_correctly_open_57ac2283").into(), Fix::Setup);
    }
    if m.contains("page view") {
        return (crate::tl_id!("voice.switch_back_to_the_page_view_to_dictate_e3deb7f8").into(), Fix::PageView);
    }
    (message.into(), Fix::None)
}

fn system_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macOS"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        crate::tl_id!("voice.your_system_3a1356ff")
    }
}

/// Whether this system has a microphone privacy page NoteCraft can open.
const MIC_SETTINGS: bool = cfg!(any(target_os = "windows", target_os = "macos"));

#[cfg(target_os = "windows")]
#[path = "voice.win.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "voice.mac.rs"]
mod platform;

/// Opens the system's microphone privacy settings.
fn open_microphone_settings() -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let opened = platform::open_microphone_settings();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opened: std::io::Result<()> = Err(std::io::Error::other("no microphone settings page on this system"));
    opened.map_err(|e| crate::i18n::format_message_id("voice.cannot_open_the_microphone_settings_error_b3a9cb13", &[("error", &e.to_string())]))
}

/// Listens without recognizing anything: the microphone check only wants the levels.
struct Listen;

impl Transcriber for Listen {
    fn capabilities(&self) -> craft_voice_core::Capabilities {
        craft_voice_core::Capabilities::default()
    }
    fn prepare(&mut self, _: &Cancellation) -> Result<(), String> {
        Ok(())
    }
    fn transcribe(&mut self, _: &PcmAudio, _: &Cancellation) -> Result<String, String> {
        Ok(String::new())
    }
}

/// What the shortcut's release means: after a hold it ends a push-to-talk recording; after a
/// tap it does nothing, and the next press ends the recording instead.
fn release_finishes(held_for: Duration) -> bool {
    held_for >= HOLD
}

/// Every non-repeat press of `key` with no modifiers in this frame's input, taken out of it so
/// the page underneath does not also see it. Auto-repeat presses are dropped too.
fn take_press(ctx: &Context, key: Key) -> bool {
    ctx.input_mut(|i| {
        let mut hit = false;
        i.events.retain(|event| match event {
            egui::Event::Key { key: k, pressed: true, repeat, modifiers, .. } if *k == key && modifiers.is_none() => {
                hit |= !*repeat;
                false
            }
            _ => true,
        });
        hit
    })
}

/// Seconds as minutes and seconds, for the recording bar.
fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.0) as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// Adds literal transcript paragraphs and keeps container block counts in step.
fn add_paragraphs(blocks: &mut Vec<notecraft_formats::Block>, boxes: &mut [notecraft_model::Container], at: usize, lines: &[&str]) {
    let at = at.min(blocks.len());
    let before = blocks.len();
    for (k, line) in lines.iter().enumerate() {
        blocks.insert(at + k, notecraft_formats::Block::Paragraph(vec![notecraft_formats::Span { text: (*line).into(), ..Default::default() }]));
    }
    if !boxes.is_empty() && boxes.iter().map(|c| c.blocks).sum::<usize>() == before {
        let owner = at.saturating_sub(1);
        let mut start = 0;
        for c in boxes.iter_mut() {
            if owner < start + c.blocks {
                c.blocks += lines.len();
                return;
            }
            start += c.blocks;
        }
        if let Some(last) = boxes.last_mut() {
            last.blocks += lines.len();
        }
    }
}

/// Adjacent spans with the same marks are one semantic run, regardless of parser chunking.
fn normalized(blocks: &[notecraft_formats::Block]) -> Vec<notecraft_formats::Block> {
    use notecraft_formats::{Block, Span};
    fn spans(spans: &mut Vec<Span>) {
        let mut joined: Vec<Span> = Vec::new();
        for span in std::mem::take(spans) {
            if span.text.is_empty() && span.image.is_none() {
                continue;
            }
            if let Some(last) = joined.last_mut() {
                let mut a = last.clone();
                a.text.clear();
                let mut b = span.clone();
                b.text.clear();
                if a == b {
                    last.text.push_str(&span.text);
                    continue;
                }
            }
            joined.push(span);
        }
        *spans = joined;
    }
    let mut blocks = blocks.to_vec();
    for block in &mut blocks {
        match block {
            Block::Paragraph(s) | Block::Heading { spans: s, .. } | Block::Quote(s) | Block::Item { spans: s, .. } => spans(s),
            Block::Table { head, rows } => {
                for cell in head.iter_mut().chain(rows.iter_mut().flatten()) {
                    spans(cell);
                }
            }
            _ => {}
        }
    }
    blocks
}

/// Parse an inert position token, then replace it inside the Span model so transcript
/// punctuation cannot become markup. Serialization is checked before the edit is applied.
fn insert_words(block: &mut notecraft_formats::Block, caret: &pageedit::Caret, words: &str) -> Result<pageedit::Caret, String> {
    let mut stable = block.clone();
    pageedit::set_caret_text(&mut stable, &caret.text);
    if normalized(&[stable]) != normalized(std::slice::from_ref(block)) {
        return Err(crate::tl_id!("voice.this_cursor_position_cannot_preserve_the_existing_ccbd81ed").into());
    }
    let words = spaced(&caret.text, caret.selection, words);
    let mut marker = "NoteCraftVoiceInsertionPoint".to_string();
    while caret.text.contains(&marker) || words.contains(&marker) {
        marker.push('X');
    }
    let (source, _) = pageedit::insert_at_caret(&caret.text, caret.selection, &marker);
    pageedit::set_caret_text(block, &source);
    let marked = pageedit::caret_text(block);
    let position = marked.find(&marker).ok_or(crate::tl_id!("voice.choose_a_cursor_position_in_the_paragraph_s_words_6538d9ab"))?;
    let at = marked[..position].chars().count() + words.chars().count();
    let spans = match block {
        notecraft_formats::Block::Paragraph(s)
        | notecraft_formats::Block::Heading { spans: s, .. }
        | notecraft_formats::Block::Quote(s)
        | notecraft_formats::Block::Item { spans: s, .. } => s,
        _ => return Err(crate::tl_id!("voice.choose_a_paragraph_before_inserting_3f802b0c").into()),
    };
    let span = spans
        .iter_mut()
        .find(|s| s.text.contains(&marker))
        .ok_or(crate::tl_id!("voice.choose_a_cursor_position_outside_formatting_marks_54a40d44"))?;
    span.text = span.text.replacen(&marker, &words, 1);
    let text = pageedit::caret_text(block);
    let at = at.min(text.chars().count());
    Ok(pageedit::Caret { text, selection: (at, at), place: Some(at), focus: true, ..caret.clone() })
}

/// The caret at the end of block `at`.
fn caret_at_end(ed: &Editor, at: usize) -> Option<pageedit::Caret> {
    let text = pageedit::caret_text(ed.blocks.get(at)?);
    let n = text.chars().count();
    Some(pageedit::Caret { block: at, text, selection: (n, n), place: Some(n), focus: true, ..Default::default() })
}

fn fingerprint(file: &Path) -> Result<u64, String> {
    let input = std::fs::File::open(file)
        .map_err(|e| crate::i18n::format_message_id("voice.cannot_read_the_page_source_error_3c53fd4d", &[("error", &e.to_string())]))?;
    let mut bytes = Vec::new();
    input.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("this page is too large for a dictation target".into());
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    Ok(hash.finish())
}

fn page_file(ed: &Editor) -> PathBuf {
    ed.dir.join(if ed.format == PageFormat::Html { "index.html" } else { "index.md" })
}

fn validate_voice_block(block: &notecraft_formats::Block) -> Result<(), String> {
    use notecraft_formats::Block;
    let spans = match block {
        Block::Paragraph(spans) | Block::Heading { spans, .. } | Block::Quote(spans) | Block::Item { spans, .. } => spans,
        _ => return Err(crate::tl_id!("voice.dictation_currently_supports_paragraphs_headings_q_4db15ad2").into()),
    };
    if spans.iter().any(|span| span.code || span.math) {
        return Err(crate::tl_id!("voice.choose_a_paragraph_without_inline_code_or_equation_bf91894f").into());
    }
    Ok(())
}

/// Below this peak level (about -40 dBFS) the microphone is taken to be hearing nothing.
const SILENT: f32 = 0.01;

/// A level from 0 to 1 as the fill of a meter that reads in decibels, from -60 dBFS to 0.
fn meter(level: f32) -> f32 {
    if level <= 0.0 { 0.0 } else { ((20.0 * level.log10() + 60.0) / 60.0).clamp(0.0, 1.0) }
}

/// What to tell the person when audio arrives but none of it is sound.
fn input_hint(seconds: f32, loudest: f32) -> Option<&'static str> {
    (seconds >= 2.0 && loudest < SILENT).then_some(crate::tl_id!("voice.no_sound_from_the_microphone_yet_check_that_the_ri_84320e7b"))
}

/// Dictation arrives as a sentence of its own. Next to words already on the line it needs the
/// space typing would have put there; next to punctuation or white space it needs none.
fn spaced(line: &str, selection: (usize, usize), text: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let (from, to) = (selection.0.min(selection.1), selection.0.max(selection.1));
    // Look past the marks of a styled neighbour to the word it holds.
    let word = |c: &&char| !matches!(**c, '*' | '~');
    let before = chars.get(..from).unwrap_or_default().iter().rev().find(word);
    let after = chars.get(to..).unwrap_or_default().iter().find(word);
    let mut out = String::with_capacity(text.len() + 2);
    if let (Some(b), Some(f)) = (before, text.chars().next())
        && !b.is_whitespace()
        && !"([{\"'\u{201c}\u{2018}\u{bf}\u{a1}/-".contains(*b)
        && f.is_alphanumeric()
    {
        out.push(' ');
    }
    out.push_str(text);
    if let (Some(a), Some(l)) = (after, text.chars().last())
        && a.is_alphanumeric()
        && !l.is_whitespace()
    {
        out.push(' ');
    }
    out
}

fn voice_cache_root(session: &Session) -> Result<PathBuf, String> {
    let parent = session.settings.path().parent().unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|_| crate::tl_id!("voice.cannot_locate_the_local_settings_folder_6346b636"))?;
    let in_notebook = |path: &Path| {
        path.starts_with(session.root())
            || session.opened_elsewhere().iter().any(|root| std::fs::canonicalize(root).is_ok_and(|root| path.starts_with(root)))
            || path.ancestors().any(|dir| notecraft_store::tree::read_node(dir).is_some_and(|node| node.kind == notecraft_model::NodeKind::Notebook))
    };
    if in_notebook(&parent) {
        return Err(crate::tl_id!("voice.keep_the_settings_folder_outside_notebooks_before_a55ba6bc").into());
    }
    let root = parent.join("voice-cache");
    std::fs::create_dir_all(&root)
        .map_err(|e| crate::i18n::format_message_id("voice.cannot_create_the_voice_cache_error_05777284", &[("error", &e.to_string())]))?;
    let root = std::fs::canonicalize(root)
        .map_err(|e| crate::i18n::format_message_id("voice.cannot_resolve_the_voice_cache_error_902cca25", &[("error", &e.to_string())]))?;
    if in_notebook(&root) {
        return Err(crate::tl_id!("voice.the_voice_cache_resolves_into_a_notebook_choose_a_1f9a56e8").into());
    }
    Ok(root)
}

impl NoteCraftApp {
    /// Where dictated words would go now: the caret when it is somewhere words can go, the end
    /// of the page otherwise. Fails only when the page cannot take words at all.
    fn capture_voice_target(&mut self) -> Result<InsertionTicket, String> {
        if self.show_source {
            return Err(crate::tl_id!("voice.return_to_the_page_view_before_dictating_2c363339").into());
        }
        let ed = self.editor.as_ref().ok_or(crate::tl_id!("voice.open_a_page_to_dictate_into_0300e9a6"))?;
        if !ed.writable {
            return Err(crate::tl_id!("voice.this_page_is_read_only_here_so_dictation_can_t_wri_47a8e613").into());
        }
        // An autosave still being written changes the file; take it in before reading the file.
        self.take_saves(true);
        let ed = self.editor.as_ref().ok_or(crate::tl_id!("voice.the_page_closed_fc708395"))?;
        let disk = notecraft_store::page::load_page(&ed.dir)
            .map_err(|_| crate::tl_id!("voice.cannot_read_the_page_from_disk_reload_it_then_dict_b17aa77a"))?;
        if disk.body != ed.disk_body {
            return Err(crate::tl_id!("voice.another_program_changed_this_page_reload_it_then_d_cc71cb4e").into());
        }
        self.save_now();
        let ed = self.editor.as_ref().ok_or(crate::tl_id!("voice.the_page_closed_fc708395"))?;
        if ed.dirty_since.is_some() {
            return Err(crate::tl_id!("voice.notecraft_couldn_t_save_the_page_so_it_won_t_add_d_5c189933").into());
        }
        let anchor = match &ed.caret {
            Some(caret)
                if ed.across.is_none_or(|a| a.inside_one_block())
                    && ed.blocks.get(caret.block).is_some_and(|b| validate_voice_block(b).is_ok())
                    && caret.selection.0.max(caret.selection.1) <= caret.text.chars().count() =>
            {
                Anchor::Caret(caret.clone())
            }
            _ => Anchor::End,
        };
        self.voice.ticket_counter =
            self.voice.ticket_counter.checked_add(1).ok_or(crate::tl_id!("voice.reopen_the_app_to_start_a_new_target_sequence_907696fc"))?;
        Ok(InsertionTicket {
            id: self.voice.ticket_counter.to_string(),
            epoch: self.voice.editor_epoch,
            generation: ed.generation,
            dir: ed.dir.clone(),
            body: ed.body.clone(),
            source: fingerprint(&page_file(ed))?,
            title: ed.title.clone(),
            caret_snapshot: ed.caret.clone(),
            across_snapshot: ed.across,
            anchor,
        })
    }

    /// Focus and pending placement are rendering details. Logical selection and page identity
    /// decide whether a recorded insertion position still describes the user's current target.
    fn cursor_matches_ticket(&self, ticket: &InsertionTicket) -> bool {
        self.editor.as_ref().is_some_and(|ed| {
            ed.across == ticket.across_snapshot
                && match (&ed.caret, &ticket.caret_snapshot) {
                    (None, None) => true,
                    (Some(current), Some(saved)) => {
                        current.block == saved.block
                            && current.text == saved.text
                            && current.selection == saved.selection
                            && current.cell == saved.cell
                            && current.span == saved.span
                    }
                    _ => false,
                }
        })
    }

    /// The page is as it was when `ticket` was taken, so its position still means the same place.
    fn ticket_is_current(&self, ticket: &InsertionTicket) -> bool {
        let Some(ed) = self.editor.as_ref() else { return false };
        let same_page = !self.show_source
            && ed.writable
            && ticket.epoch == self.voice.editor_epoch
            && ticket.dir == ed.dir
            && ticket.generation == ed.generation
            && ticket.body == ed.body
            && fingerprint(&page_file(ed)).is_ok_and(|f| f == ticket.source);
        same_page
            && self.cursor_matches_ticket(ticket)
            && match &ticket.anchor {
                Anchor::End => true,
                Anchor::Caret(caret) => {
                    ed.blocks.get(caret.block).is_some_and(|b| pageedit::caret_text(b) == caret.text && validate_voice_block(b).is_ok())
                }
            }
    }

    fn apply_voice_text(&mut self, ticket: &InsertionTicket, text: &str) -> Result<(), String> {
        if text.len() > craft_voice_core::MAX_TRANSCRIPT_BYTES {
            return Err(crate::tl_id!("voice.the_words_are_longer_than_64_kib_shorten_them_befo_74a29916").into());
        }
        // Each line becomes a paragraph of its own.
        let lines: Vec<&str> = text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect();
        let Some((first, rest)) = lines.split_first() else {
            return Err(crate::tl_id!("voice.there_are_no_words_to_insert_d397d6a5").into());
        };
        if !self.ticket_is_current(ticket) {
            return Err(crate::tl_id!("voice.the_page_changed_while_you_were_talking_click_wher_2d22a184").into());
        }
        let ed = self.editor.as_mut().ok_or(crate::tl_id!("voice.the_page_closed_fc708395"))?;
        // History stores blocks/containers, but not indexed paragraph/table layout. Refuse
        // the narrow case that would shift those indexes rather than corrupt unrelated layout.
        if let Anchor::Caret(caret) = &ticket.anchor
            && !rest.is_empty()
        {
            let end = caret.selection.0.max(caret.selection.1);
            if caret.text.chars().count() > end {
                return Err(crate::tl_id!("voice.multiline_needs_line_end").into());
            }

            let at = caret.block + 1;
            let mut shifted = ed.columns.clone();
            if shifted.insert_block(at) || ed.justified.blocks().iter().any(|(block, _)| *block >= at) {
                return Err(crate::tl_id!("voice.layout_would_shift").into());
            }
        }
        let mut blocks = ed.blocks.clone();
        let mut boxes = ed.boxes.clone();
        let before_caret = ed.caret.clone();
        let mut next_caret = before_caret.clone();
        let last = match &ticket.anchor {
            Anchor::Caret(caret) => {
                let block = blocks.get_mut(caret.block).ok_or(crate::tl_id!("voice.the_paragraph_closed_868b6d17"))?;
                let tag = pageedit::tag_of(block).0;
                next_caret = Some(insert_words(block, caret, first)?);
                if pageedit::tag_of(block).0 != tag {
                    return Err(crate::tl_id!("voice.these_words_would_become_a_page_tag_edit_the_tag_m_bfed5fa8").into());
                }
                add_paragraphs(&mut blocks, &mut boxes, caret.block + 1, rest);
                (!rest.is_empty()).then_some(caret.block + rest.len())
            }
            Anchor::End => {
                let at = blocks.len();
                add_paragraphs(&mut blocks, &mut boxes, at, &lines);
                Some(at + lines.len() - 1)
            }
        };
        let first_new = match &ticket.anchor {
            Anchor::Caret(c) => c.block + 1,
            Anchor::End => ed.blocks.len(),
        };
        if blocks
            .iter()
            .skip(first_new)
            .take(if matches!(ticket.anchor, Anchor::End) { lines.len() } else { rest.len() })
            .any(|block| pageedit::tag_of(block).0.is_some())
        {
            return Err(crate::tl_id!("voice.these_words_would_become_a_page_tag_edit_the_tag_m_bfed5fa8").into());
        }
        let arrangement = (ed.arranged && !boxes.is_empty()).then_some(boxes.as_slice());
        let body = notecraft_formats::boxes::write(ed.format, &blocks, arrangement);
        let (read_back, read_boxes) = notecraft_formats::boxes::read(ed.format, &body);
        if normalized(&read_back) != normalized(&blocks) || read_boxes.as_deref() != arrangement {
            return Err(crate::tl_id!("voice.this_page_format_cannot_preserve_these_literal_wor_fc42673c").into());
        }
        ed.history.remember_caret(match &ticket.anchor {
            Anchor::Caret(c) => Some(c.clone()),
            Anchor::End => before_caret,
        });
        ed.blocks = blocks;
        ed.boxes = boxes;
        ed.body = body;
        ed.caret = next_caret;
        if let Some(last) = last {
            ed.caret = caret_at_end(ed, last);
        }
        Self::mark_dirty_from_blocks(ed);
        ed.history.record(crate::history::Step::new(
            ed.body.clone(),
            ed.blocks.clone(),
            ed.boxes.clone(),
            ed.caret.clone(),
            crate::history::Kind::Edit,
        ));
        Ok(())
    }

    fn voice_command(&mut self, command: &str, p: &Params) -> Result<(), String> {
        let output = self.session.run_with_origin(command, p, InvocationOrigin::LocalUi).map_err(|e| e.to_string())?;
        match output {
            Output::Voice(VoiceEffect::Start { session, .. }) => {
                self.voice.review.clear();
                self.voice.review_revision = 0;
                self.voice.seconds = 0.0;
                self.voice.level = 0.0;
                self.voice.loudest = 0.0;
                self.voice.pending_stop = false;
                self.voice.editing = false;
                self.voice.auto_tried = false;
                let setup = (|| -> Result<Runtime, String> {
                    let root = voice_cache_root(&self.session)?;
                    let mut config = WhisperConfig::new(self.voice.executable.clone().into(), self.voice.model.clone().into(), root);
                    config.language = self.voice.language.clone();
                    Runtime::start(session, Box::new(config), self.voice.device.clone())
                })();
                match setup {
                    Ok(runtime) => self.voice.runtime = Some(runtime),
                    Err(message) => {
                        let _ = self.session.voice_update(VoiceUpdate::Failed(session, message.clone()));
                        return Err(message);
                    }
                }
            }
            Output::Voice(VoiceEffect::Stop { .. }) => {
                if let Some(runtime) = &self.voice.runtime {
                    runtime.stop();
                }
            }
            Output::Voice(VoiceEffect::Cancel { .. }) => {
                self.voice.runtime.take();
                self.voice.target = None;
                self.voice.review.clear();
                self.voice.editing = false;
                self.voice.pressed_at = None;
                self.voice.pending_stop = false;
            }
            Output::Voice(VoiceEffect::Commit { session, target, text, .. }) => {
                let result = match self.voice.target.clone().filter(|t| t.id == target) {
                    Some(ticket) => self.apply_voice_text(&ticket, &text),
                    None => Err(crate::tl_id!("voice.click_where_the_words_should_go_and_choose_insert_473a713a").into()),
                };
                let _ = self.session.voice_update(VoiceUpdate::Applied(session, result.clone()));
                result?;
                self.voice.error = None;
                self.voice.runtime.take();
                self.voice.target = None;
                self.voice.review.clear();
                self.voice.editing = false;
            }
            _ => {}
        }
        Ok(())
    }

    fn start_voice(&mut self) -> Result<(), String> {
        let ticket = self.capture_voice_target()?;
        let id = ticket.id.clone();
        self.voice.target = Some(ticket);
        self.voice.error = None;
        self.voice_command("voice.start", &params(&[("target", &id)]))
    }

    fn stop_voice(&mut self) -> Result<(), String> {
        if self.session.voice().state() == State::Preparing {
            self.voice.pending_stop = true;
            return Ok(());
        }
        if self.session.voice().state() != State::Recording {
            return Ok(());
        }
        let session = self.session.voice().id().to_string();
        self.voice_command("voice.stop", &params(&[("session", &session)]))
    }

    /// The title of the page the words were for, when a different page (or none) is open now.
    pub(super) fn target_elsewhere(&self) -> Option<String> {
        let ticket = self.voice.target.as_ref()?;
        (self.editor.as_ref().map(|ed| &ed.dir) != Some(&ticket.dir)).then(|| ticket.title.clone())
    }

    fn commit_voice(&mut self) -> Result<(), String> {
        self.commit_with(false)
    }

    /// Insert on the open page although the words were for another one: the person chose this.
    fn commit_on_this_page(&mut self) -> Result<(), String> {
        self.commit_with(true)
    }

    fn commit_with(&mut self, other_page: bool) -> Result<(), String> {
        if !other_page && let Some(title) = self.target_elsewhere() {
            return Err(crate::i18n::format_message_id("voice.these_words_were_for_the_page_title_choose_insert_27760264", &[("title", &title)]));
        }
        let current = self.session.voice();
        if current.state() != State::Review || self.voice.review_revision != current.revision() || self.voice.review != current.draft() {
            return Err(crate::tl_id!("voice.the_displayed_draft_is_not_the_accepted_revision_r_3002e242").into());
        }
        // The page changed while the words were being spoken or reviewed: rather than refuse, put
        // them where the person is now, which is where they are looking.
        let mut moved = false;
        if let Some(ticket) = self.voice.target.clone()
            && !self.ticket_is_current(&ticket)
        {
            let fresh = self.capture_voice_target()?;
            let session = self.session.voice().id().to_string();
            self.voice_command("voice.retarget", &params(&[("session", &session), ("target", &fresh.id)]))?;
            self.voice.target = Some(fresh);
            self.voice.review_revision = self.session.voice().revision();
            moved = true;
        }
        let current = self.session.voice();
        let session = current.id().to_string();
        let revision = current.revision().to_string();
        self.voice_command("voice.commit", &params(&[("session", &session), ("revision", &revision)]))?;
        self.say(if moved {
            crate::tl_id!("voice.the_page_changed_while_you_were_talking_so_the_wor_41829e1a")
        } else {
            crate::tl_id!("voice.dictation_inserted_ctrl_z_takes_it_back_a6eb393d")
        });
        Ok(())
    }

    fn cancel_voice(&mut self) {
        let session = self.session.voice().id().to_string();
        let _ = self.voice_command("voice.cancel", &params(&[("session", &session)]));
        self.voice.runtime.take();
        self.voice.review.clear();
        self.voice.target = None;
        self.voice.editing = false;
        self.voice.pressed_at = None;
        self.voice.pending_stop = false;
    }

    /// The program and model are chosen and exist, so recording can start.
    fn dictation_ready(&self) -> bool {
        Path::new(&self.voice.executable).is_file() && Path::new(&self.voice.model).metadata().is_ok_and(|m| m.is_file() && m.len() > 0)
    }

    /// Starts dictating, or opens setup first when the recognizer is not chosen yet.
    fn begin_dictation(&mut self) {
        self.voice.error = None;
        if !self.dictation_ready() {
            self.voice.setup_open = true;
            self.voice.setup_reason = Some(crate::tl_id!("voice.choose_the_speech_recognizer_and_a_model_to_start_4a44889b"));
            return;
        }
        self.voice.mic_test = None;
        self.voice.recognizer_test = None;
        self.voice.setup_open = false;
        if let Err(e) = self.start_voice() {
            self.voice.error = Some(e);
        }
    }

    fn finish_dictation(&mut self) {
        if let Err(e) = self.stop_voice() {
            self.voice.error = Some(e);
        }
    }

    /// What the mic button and the shortcut do: start when idle, finish while listening.
    pub(super) fn toggle_dictation(&mut self) {
        match self.session.voice().state() {
            State::Preparing | State::Recording => self.finish_dictation(),
            State::Idle | State::Complete | State::Cancelled | State::Failed => self.begin_dictation(),
            // The preview holds the choices while words are finishing or waiting for review.
            State::Finalizing | State::Review | State::Applying => {}
        }
    }

    pub(super) fn dictation_mic(&self) -> crate::ribbon::Mic {
        match self.session.voice().state() {
            State::Preparing | State::Recording => crate::ribbon::Mic::Live,
            State::Finalizing | State::Review | State::Applying => crate::ribbon::Mic::Busy,
            State::Idle | State::Complete | State::Cancelled | State::Failed => crate::ribbon::Mic::Ready,
        }
    }

    pub(super) fn open_dictation_setup(&mut self) {
        self.voice.setup_open = true;
        self.voice.setup_reason = None;
    }

    /// The shortcut, Enter and Esc, taken before the page sees them: while words are being
    /// spoken, Enter must not split the paragraph the words are going into.
    fn dictation_keys(&mut self, ctx: &Context) {
        let state = self.session.voice().state();
        if let Some(key) = Key::from_name(&self.voice.shortcut) {
            if take_press(ctx, key) && (state.active() || !(self.voice.setup_open || self.dialog.is_some() || self.backstage.is_some())) {
                let live = matches!(state, State::Preparing | State::Recording);
                self.toggle_dictation();
                self.voice.pressed_at = if live { None } else { Some(Instant::now()) };
            }
            // A hold ends when the key comes up, when its release was missed (the key is simply not
            // down any more), or when the window loses focus with the key held: another window
            // would get the release, and the microphone would stay open.
            if let Some(at) = self.voice.pressed_at {
                let (released, focused) = ctx.input(|i| (i.key_released(key) || !i.key_down(key), i.focused));
                if released || !focused {
                    self.voice.pressed_at = None;
                    if !focused || release_finishes(at.elapsed()) {
                        self.finish_dictation();
                    }
                }
            }
        }
        if matches!(state, State::Preparing | State::Recording | State::Finalizing) {
            if take_press(ctx, Key::Escape) {
                self.cancel_voice();
            } else if state != State::Finalizing && take_press(ctx, Key::Enter) {
                self.finish_dictation();
            }
        }
    }

    pub(super) fn poll_voice(&mut self, ctx: &Context) {
        self.dictation_keys(ctx);
        let events = self.voice.runtime.as_mut().map(Runtime::poll).unwrap_or_default();
        let mut finished = false;
        for event in events {
            let update = match event {
                RuntimeEvent::Ready { session } => Some(VoiceUpdate::Ready(session)),
                RuntimeEvent::Stopped { session } => Some(VoiceUpdate::Stopped(session)),
                RuntimeEvent::Progress { session, seconds, level } => {
                    if session == self.session.voice().id() {
                        self.voice.seconds = seconds;
                        self.voice.level = level;
                        self.voice.loudest = self.voice.loudest.max(level);
                    }
                    None
                }
                RuntimeEvent::Transcript(t) => {
                    finished |= t.is_final;
                    Some(VoiceUpdate::Transcript(t))
                }
                RuntimeEvent::Error { session, message } => {
                    finished = true;
                    self.voice.error = Some(message.clone());
                    Some(VoiceUpdate::Failed(session, message))
                }
            };
            if let Some(update) = update {
                let _ = self.session.voice_update(update);
            }
        }
        if finished {
            self.voice.runtime.take();
        }
        if self.voice.pending_stop && self.session.voice().state() == State::Recording {
            self.voice.pending_stop = false;
            self.finish_dictation();
        }
        if self.voice.review_revision != self.session.voice().revision() && !self.voice.editing {
            self.voice.review = self.session.voice().draft().into();
            self.voice.review_revision = self.session.voice().revision();
        }
        self.auto_insert();
        self.poll_setup_checks();
        if self.voice.runtime.is_some() || self.voice.mic_test.is_some() || self.voice.recognizer_test.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    /// With review turned off, finished words go straight into the page; Undo takes them back.
    fn auto_insert(&mut self) {
        let voice = self.session.voice();
        // Only words from a finished final pass go in by themselves. Words left over from a recording
        // that failed (a disconnected microphone, say) are an unfinished preview: they wait for review.
        if self.voice.review_first
            || self.voice.auto_tried
            || voice.state() != State::Review
            || voice.error().is_some()
            || voice.draft().trim().is_empty()
        {
            return;
        }
        self.voice.auto_tried = true;
        if self.dialog.is_some()
            || self.backstage.is_some()
            || self.voice.setup_open
            || !self.voice.target.as_ref().is_some_and(|ticket| self.ticket_is_current(ticket))
        {
            self.voice.error = Some(crate::tl_id!("voice.target_changed_review_required").into());
            return;
        }
        if let Err(e) = self.commit_voice() {
            self.voice.error = Some(e);
        }
    }

    fn start_mic_test(&mut self) {
        self.voice.mic_result = None;
        self.voice.mic_test = None;
        match Runtime::start(MIC_TEST, Box::new(Listen), self.voice.device.clone()) {
            Ok(runtime) => self.voice.mic_test = Some(MicTest { runtime, started: Instant::now(), level: 0.0, loudest: 0.0, heard_audio: false }),
            Err(e) => self.voice.mic_result = Some(Err(explain(&e).0)),
        }
    }

    fn start_recognizer_test(&mut self) {
        self.voice.recognizer_result = None;
        self.voice.recognizer_test = None;
        let started = (|| -> Result<Runtime, String> {
            let root = voice_cache_root(&self.session)?;
            let mut config = WhisperConfig::new(self.voice.executable.clone().into(), self.voice.model.clone().into(), root);
            config.language = self.voice.language.clone();
            // A second of silence: enough for the recognizer to load the model and answer.
            let silence = PcmAudio::new(vec![0; craft_voice_runtime::audio::SAMPLE_RATE])?;
            Runtime::from_audio(RECOGNIZER_TEST, Box::new(config), silence)
        })();
        match started {
            Ok(runtime) => self.voice.recognizer_test = Some(runtime),
            Err(e) => self.voice.recognizer_result = Some(Err(explain(&e).0)),
        }
    }

    fn poll_setup_checks(&mut self) {
        if let Some(test) = self.voice.mic_test.as_mut() {
            let mut failed = None;
            for event in test.runtime.poll() {
                match event {
                    RuntimeEvent::Ready { .. } => test.heard_audio = true,
                    RuntimeEvent::Progress { level, .. } => {
                        test.heard_audio = true;
                        test.level = level;
                        test.loudest = test.loudest.max(level);
                    }
                    RuntimeEvent::Error { message, .. } => failed = Some(explain(&message).0),
                    _ => {}
                }
            }
            if failed.is_some() || test.started.elapsed() >= MIC_TEST_FOR {
                let result = match failed {
                    Some(why) => Err(why),
                    None if !test.heard_audio => Err(crate::tl_id!("voice.no_audio_arrived_from_the_microphone_check_the_sel_68498934").into()),
                    None if test.loudest < SILENT => Err(crate::tl_id!("voice.the_microphone_sends_audio_but_it_is_silent_check_d7cf0faa").into()),
                    None => Ok(crate::i18n::format_message_id(
                        "voice.heard_you_loudest_sound_decibels_db_ce75d644",
                        &[("decibels", &format!("{:.0}", 20.0 * test.loudest.log10()))],
                    )),
                };
                self.voice.mic_result = Some(result);
                self.voice.mic_test = None;
            }
        }
        if let Some(runtime) = self.voice.recognizer_test.as_mut() {
            let mut result = None;
            for event in runtime.poll() {
                match event {
                    RuntimeEvent::Transcript(t) if t.is_final => {
                        result = Some(Ok(crate::tl_id!("voice.the_recognizer_and_the_model_work_11e481ee").to_string()))
                    }
                    RuntimeEvent::Error { message, .. } => result = Some(Err(explain(&message).0)),
                    _ => {}
                }
            }
            if let Some(result) = result {
                self.voice.recognizer_result = Some(result);
                self.voice.recognizer_test = None;
            }
        }
    }

    fn save_voice_settings(&mut self) {
        let review = if self.voice.review_first { "true" } else { "false" };
        for (key, value) in [
            ("voice_executable", self.voice.executable.as_str()),
            ("voice_model", self.voice.model.as_str()),
            ("voice_language", self.voice.language.as_str()),
            ("voice_device", self.voice.device.as_deref().unwrap_or("")),
            ("voice_shortcut", self.voice.shortcut.as_str()),
            ("voice_review", review),
        ] {
            if let Err(error) = self.session.settings.set(key, value) {
                self.voice.error =
                    Some(crate::i18n::format_message_id("voice.cannot_save_dictation_settings_error_fa8703c7", &[("error", &error.to_string())]));
                break;
            }
        }
    }

    fn submit_voice_edit(&mut self, action: &mut Option<&str>) {
        let id = self.session.voice().id().to_string();
        let revision = self.session.voice().revision().to_string();
        let text = self.voice.review.clone();
        match self.voice_command("voice.draft.set", &params(&[("session", &id), ("revision", &revision), ("text", &text)])) {
            Ok(()) => self.voice.review_revision = self.session.voice().revision(),
            Err(e) => {
                self.voice.error = Some(e);
                // Keep the rejected words visible. Approval of an older revision needs a
                // later rendered review and a new explicit Insert action.
                if *action == Some("commit") {
                    *action = None;
                }
            }
        }
    }

    fn act_on_fix(&mut self, fix: Fix) {
        match fix {
            Fix::MicSettings => {
                if let Err(e) = open_microphone_settings() {
                    self.voice.error = Some(e);
                }
            }
            Fix::Setup => self.open_dictation_setup(),
            Fix::PageView => {
                self.show_source = false;
                self.voice.error = None;
            }
            Fix::None => {}
        }
    }
}

// i18n-ui-text: label
fn primary(label: &str, t: &Tokens) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_string()).strong().color(t.band_text)).fill(t.band)
}

/// A level meter that reads in decibels: a track with the level filled in.
fn level_bar(ui: &mut Ui, level: f32, t: &Tokens) {
    let (rect, resp) = ui.allocate_exact_size(vec2(110.0, 8.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, t.faint);
    let fill = meter(level);
    if fill > 0.0 {
        let lit = egui::Rect::from_min_size(rect.min, vec2(rect.width() * fill, rect.height()));
        painter.rect_filled(lit, 4.0, if fill > 0.92 { t.warn } else { t.ok });
    }
    resp.on_hover_text(crate::tl_id!("voice.microphone_level_7b9bf79a"));
}

impl NoteCraftApp {
    pub(super) fn show_voice(&mut self, ctx: &Context) {
        self.dictation_bar(ctx);
        self.dictation_preview(ctx);
        if self.voice.setup_open {
            self.dictation_setup(ctx);
        }
    }

    /// The bar along the bottom while dictation is listening or finishing, and for errors.
    fn dictation_bar(&mut self, ctx: &Context) {
        let state = self.session.voice().state();
        let busy = matches!(state, State::Preparing | State::Recording | State::Finalizing);
        // An error during review is shown with the words, in the preview.
        let error = if state == State::Review { None } else { self.voice.error.clone() };
        if !busy && error.is_none() {
            return;
        }
        let t = self.tokens;
        let mut action: Option<&str> = None;
        let mut fix = Fix::None;
        egui::Area::new(egui::Id::new("dictation-bar")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0)).show(
            ctx,
            |ui| {
                egui::Frame::NONE
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.0, if state == State::Recording { t.warn } else { t.accent }))
                    .corner_radius(20.0)
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: egui::Color32::from_black_alpha(60) })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            match state {
                                State::Preparing => {
                                    ui.add(egui::Spinner::new().size(14.0));
                                    ui.label(egui::RichText::new(crate::tl_id!("voice.starting_the_microphone_26a22bd2")).color(t.text));
                                }
                                State::Recording => {
                                    let (dot, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                    let pulse = 0.55 + 0.45 * (ui.input(|i| i.time) as f32 * 4.0).sin().abs();
                                    ui.painter().circle_filled(dot.center(), 6.0, t.warn.gamma_multiply(pulse));
                                    ui.label(egui::RichText::new(crate::tl_id!("voice.listening_dc353487")).strong().color(t.strong));
                                    level_bar(ui, self.voice.level, &t);
                                    ui.label(egui::RichText::new(clock(self.voice.seconds)).monospace().color(t.muted));
                                }
                                State::Finalizing => {
                                    ui.add(egui::Spinner::new().size(14.0));
                                    ui.label(egui::RichText::new(crate::tl_id!("voice.finishing_90218af7")).color(t.text));
                                }
                                _ => {
                                    if let Some(message) = &error {
                                        let (words, f) = explain(message);
                                        fix = f;
                                        ui.add(egui::Label::new(egui::RichText::new(words).color(t.warn)).wrap()).on_hover_text(message);
                                    }
                                }
                            }
                            if matches!(state, State::Preparing | State::Recording)
                                && ui
                                    .add(primary(crate::tl_id!("voice.done_e9b450d1"), &t))
                                    .on_hover_text(crate::i18n::format_message_id(
                                        "voice.enter_or_shortcut_25ef81f2",
                                        &[("shortcut", &self.voice.shortcut)],
                                    ))
                                    .clicked()
                            {
                                action = Some("done");
                            }
                            if busy && ui.button(crate::tl_id!("ui.cancel")).on_hover_text(crate::tl_id!("voice.esc_1f7a4f9e")).clicked() {
                                action = Some("cancel");
                            }
                            if !busy {
                                let label = match fix {
                                    Fix::MicSettings => Some(crate::tl_id!("voice.microphone_settings_2a04af24")),
                                    Fix::Setup => Some(crate::tl_id!("voice.dictation_setup_b70a0aa8")),
                                    Fix::PageView => Some(crate::tl_id!("voice.page_view_6a49efac")),
                                    Fix::None => None,
                                };
                                if let Some(label) = label
                                    && ui.add(primary(label, &t)).clicked()
                                {
                                    action = Some("fix");
                                }
                                if ui.button(crate::tl_id!("voice.dismiss_70afe9ef")).clicked() {
                                    action = Some("dismiss");
                                }
                            }
                        });
                        if state == State::Recording
                            && let Some(hint) = input_hint(self.voice.seconds, self.voice.loudest)
                        {
                            ui.label(egui::RichText::new(hint).size(12.0).color(t.warn));
                        }
                    });
            },
        );
        if busy {
            ctx.request_repaint();
        }
        match action {
            Some("done") => self.finish_dictation(),
            Some("cancel") => self.cancel_voice(),
            Some("fix") => self.act_on_fix(fix),
            Some("dismiss") => self.voice.error = None,
            _ => {}
        }
    }

    /// Where the preview sits: under the line the words go into, or under the end of the page.
    fn preview_anchor(&self, ctx: &Context) -> Option<(egui::Pos2, f32)> {
        let ticket = self.voice.target.as_ref()?;
        let ed = self.editor.as_ref()?;
        if ticket.epoch != self.voice.editor_epoch
            || ticket.dir != ed.dir
            || ticket.generation != ed.generation
            || !self.cursor_matches_ticket(ticket)
        {
            return None;
        }
        let pass = ctx.cumulative_pass_nr();
        match self.voice.target.as_ref().map(|t| &t.anchor)? {
            Anchor::Caret(caret) => {
                let (at, column, drawn, block) = ctx.data(|d| d.get_temp::<(egui::Rect, egui::Rect, u64, usize)>(pageedit::caret_anchor_id()))?;
                (drawn == pass && block == caret.block).then(|| (egui::pos2(column.left(), at.bottom() + 6.0), column.width()))
            }
            Anchor::End => {
                let (below, drawn) = ctx.data(|d| d.get_temp::<(egui::Rect, u64)>(pageedit::page_end_anchor_id()))?;
                (drawn == pass).then(|| (egui::pos2(below.left(), below.top() + 6.0), below.width()))
            }
        }
    }

    /// The words as they come in, at the place they will go; then, for review, the same words
    /// with Insert, Edit and Discard.
    fn dictation_preview(&mut self, ctx: &Context) {
        let state = self.session.voice().state();
        let draft = self.session.voice().draft().to_string();
        let showing = match state {
            State::Recording | State::Finalizing => !draft.trim().is_empty(),
            State::Review => true,
            _ => false,
        };
        if !showing {
            return;
        }
        let t = self.tokens;
        let at_end = matches!(self.voice.target.as_ref().map(|t| &t.anchor), Some(Anchor::End));
        let area = egui::Area::new(egui::Id::new("dictation-preview")).order(egui::Order::Foreground);
        let (area, width) = match self.preview_anchor(ctx) {
            Some((pos, width)) => (area.fixed_pos(pos), width.clamp(280.0, 620.0)),
            None => (area.anchor(Align2::CENTER_BOTTOM, vec2(0.0, -100.0)), 520.0),
        };
        let mut action: Option<&str> = None;
        let mut edited = false;
        let mut fix = Fix::None;
        let elsewhere = self.target_elsewhere();
        area.show(ctx, |ui| {
            let review = state == State::Review;
            let fill = if review { t.panel.lerp_to_gamma(t.accent, 0.10) } else { t.panel };
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, t.accent.gamma_multiply(if review { 1.0 } else { 0.5 })))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::same(12))
                .shadow(egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: egui::Color32::from_black_alpha(45) })
                .show(ui, |ui| {
                    ui.set_width(width);
                    let place =
                        if at_end { crate::tl_id!("voice.at_the_end_of_the_page_24302b0e") } else { crate::tl_id!("voice.at_your_cursor_e3b613bb") };
                    if !review {
                        ui.label(
                            egui::RichText::new(crate::i18n::format_message_id("voice.preview_going_place_2eeaa180", &[("place", place)]))
                                .size(11.5)
                                .color(t.muted),
                        );
                        ui.add(egui::Label::new(egui::RichText::new(&draft).italics().color(t.muted)).wrap());
                        return;
                    }
                    if self.voice.review.trim().is_empty() && !self.voice.editing {
                        ui.label(egui::RichText::new(crate::tl_id!("voice.no_speech_was_recognized_7ff9a913")).strong().color(t.strong));
                        ui.label(egui::RichText::new(crate::tl_id!("voice.try_again_a_little_closer_to_the_microphone_30f96ada")).color(t.muted));
                        ui.horizontal(|ui| {
                            if ui.add(primary(crate::tl_id!("voice.try_again_042c862e"), &t)).clicked() {
                                action = Some("again");
                            }
                            if ui.button(crate::tl_id!("voice.discard_36fff63c")).clicked() {
                                action = Some("discard");
                            }
                        });
                        return;
                    }
                    let caption = match &elsewhere {
                        Some(title) => crate::i18n::format_message_id("voice.these_words_were_for_the_page_title_c7a50f95", &[("title", title)]),
                        None => crate::i18n::format_message_id("voice.ready_to_insert_place_7f3f4d09", &[("place", place)]),
                    };
                    ui.label(egui::RichText::new(caption).size(11.5).color(t.muted));
                    if self.voice.editing {
                        let label = ui.label(egui::RichText::new(crate::tl_id!("voice.dictated_words_8c96e44a")).size(11.5).color(t.muted)).id;
                        edited = ui
                            .add(
                                egui::TextEdit::multiline(&mut self.voice.review)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY)
                                    .char_limit(craft_voice_core::MAX_TRANSCRIPT_BYTES),
                            )
                            .labelled_by(label)
                            .changed();
                    } else {
                        ui.add(egui::Label::new(egui::RichText::new(&self.voice.review).size(15.0).color(t.strong)).wrap());
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (label, choice) = if elsewhere.is_some() {
                            (crate::tl_id!("voice.insert_on_this_page_f7f93d5b"), "move")
                        } else {
                            (crate::tl_id!("voice.insert_here_1fe34d33"), "commit")
                        };
                        if ui.add_enabled(!self.voice.review.trim().is_empty(), primary(label, &t)).clicked() {
                            action = Some(choice);
                        }
                        let edit = if self.voice.editing { crate::tl_id!("voice.done_editing_b4938456") } else { crate::tl_id!("ui.edit") };
                        if ui.button(edit).clicked() {
                            action = Some("edit");
                        }
                        if ui.button(crate::tl_id!("voice.discard_36fff63c")).clicked() {
                            action = Some("discard");
                        }
                    });
                    if let Some(message) = self
                        .voice
                        .error
                        .clone()
                        .or_else(|| self.session.voice().error().map(str::to_string))
                        .filter(|m| elsewhere.is_none() || !m.contains(crate::tl_id!("voice.insert_on_this_page_f7f93d5b")))
                    {
                        let (words, f) = explain(&message);
                        fix = f;
                        ui.add(egui::Label::new(egui::RichText::new(words).size(12.0).color(t.warn)).wrap());
                        if f == Fix::MicSettings && ui.button(crate::tl_id!("voice.microphone_settings_2a04af24")).clicked() {
                            action = Some("fix");
                        }
                    }
                });
        });
        if edited {
            self.submit_voice_edit(&mut action);
        }
        match action {
            Some("commit") => {
                if let Err(e) = self.commit_voice() {
                    self.voice.error = Some(e);
                }
            }
            Some("move") => {
                if let Err(e) = self.commit_on_this_page() {
                    self.voice.error = Some(e);
                }
            }
            Some("edit") => self.voice.editing = !self.voice.editing,
            Some("discard") => {
                self.cancel_voice();
                self.say(crate::tl_id!("voice.dictation_discarded_2159761f"));
            }
            Some("again") => {
                self.cancel_voice();
                self.begin_dictation();
            }
            Some("fix") => self.act_on_fix(fix),
            _ => {}
        }
    }

    /// Choosing the recognizer, model, language and microphone, with a check for each half.
    fn dictation_setup(&mut self, ctx: &Context) {
        if !self.voice.devices_listed {
            self.voice.devices_listed = true;
            match craft_voice_runtime::microphone_devices() {
                Ok(names) => self.voice.devices = names,
                Err(error) => self.voice.mic_result = Some(Err(explain(&error).0)),
            }
        }
        let t = self.tokens;
        let mut open = true;
        let mut changed = false;
        let mut action: Option<&str> = None;
        let busy = self.session.voice().state().active();
        egui::Window::new(crate::tl_id!("voice.dictation_setup_b70a0aa8"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .frame(modal_frame(&t))
            .show(ctx, |ui| {
                if let Some(reason) = self.voice.setup_reason {
                    ui.label(egui::RichText::new(reason).strong().color(t.accent));
                    ui.add_space(4.0);
                }
                ui.label(crate::tl_id!("voice.dictation_runs_on_this_computer_with_whisper_cpp_c_fe751c46"));
                ui.hyperlink_to(crate::tl_id!("voice.where_to_get_whisper_cli_and_a_model_3b6c0fcb"), WHISPER_HELP_URL);
                ui.add_space(8.0);
                ui.add_enabled_ui(!busy, |ui| {
                    let found = |ok: bool| {
                        if ok {
                            egui::RichText::new(crate::tl_id!("voice.found_bbba8413")).color(t.ok)
                        } else {
                            egui::RichText::new(crate::tl_id!("voice.not_found_475c8486")).color(t.warn)
                        }
                    };
                    let program = ui.label(egui::RichText::new(crate::tl_id!("voice.recognizer_program_whisper_cli_eb53d9da")).strong()).id;
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut self.voice.executable).desired_width(300.0)).labelled_by(program).changed();
                        if ui.button(crate::tl_id!("voice.browse_c1b20f52")).clicked()
                            && let Some(path) = rfd::FileDialog::new().set_title(crate::tl_id!("voice.choose_whisper_cli_db09c1c2")).pick_file()
                        {
                            self.voice.executable = path.to_string_lossy().into_owned();
                            changed = true;
                        }
                        ui.label(found(Path::new(&self.voice.executable).is_file()));
                    });
                    let model = ui.label(egui::RichText::new(crate::tl_id!("voice.speech_model_3c5f0bfe")).strong()).id;
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut self.voice.model).desired_width(300.0)).labelled_by(model).changed();
                        if ui.button(crate::tl_id!("voice.browse_c1b20f52")).clicked()
                            && let Some(path) = rfd::FileDialog::new().set_title(crate::tl_id!("voice.choose_a_speech_model_f18c2ce5")).pick_file()
                        {
                            self.voice.model = path.to_string_lossy().into_owned();
                            changed = true;
                        }
                        ui.label(found(Path::new(&self.voice.model).is_file()));
                    });
                    ui.label(
                        egui::RichText::new(crate::tl_id!("voice.models_whose_name_ends_in_en_understand_english_on_41b266d0"))
                            .size(11.5)
                            .color(t.muted),
                    );
                    ui.add_space(6.0);
                    egui::Grid::new("dictation-setup-choices").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                        ui.label(egui::RichText::new(crate::tl_id!("voice.language_89b86ab0")).strong());
                        let current = LANGUAGES
                            .iter()
                            .find(|(code, _)| *code == self.voice.language)
                            .map_or_else(|| self.voice.language.as_str(), |(_, name)| crate::i18n::t_id(name));
                        egui::ComboBox::from_id_salt("dictation-language").selected_text(current).show_ui(ui, |ui| {
                            for (code, name) in LANGUAGES {
                                changed |= ui.selectable_value(&mut self.voice.language, code.to_string(), crate::i18n::t_id(name)).changed();
                            }
                        });
                        ui.end_row();
                        ui.label(egui::RichText::new(crate::tl_id!("voice.microphone_24280537")).strong());
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("dictation-device")
                                .selected_text(self.voice.device.as_deref().unwrap_or(crate::tl_id!("voice.system_default_9d8d3808")))
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    changed |=
                                        ui.selectable_value(&mut self.voice.device, None, crate::tl_id!("voice.system_default_9d8d3808")).changed();
                                    for name in &self.voice.devices {
                                        changed |= ui.selectable_value(&mut self.voice.device, Some(name.clone()), name).changed();
                                    }
                                });
                            if ui.button(crate::tl_id!("voice.refresh_56e3badc")).clicked() {
                                match craft_voice_runtime::microphone_devices() {
                                    Ok(names) => self.voice.devices = names,
                                    Err(e) => self.voice.mic_result = Some(Err(explain(&e).0)),
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(egui::RichText::new(crate::tl_id!("voice.shortcut_e2012dea")).strong());
                        egui::ComboBox::from_id_salt("dictation-shortcut").selected_text(self.voice.shortcut.clone()).show_ui(ui, |ui| {
                            for key in ["F8", "F9", "F10"] {
                                changed |= ui.selectable_value(&mut self.voice.shortcut, key.into(), key).changed();
                            }
                        });
                        ui.end_row();
                    });
                    ui.label(
                        egui::RichText::new(crate::i18n::format_message_id(
                            "voice.tap_shortcut_to_start_and_again_to_finish_or_hold_6f592a2d",
                            &[("shortcut", &self.voice.shortcut)],
                        ))
                        .size(11.5)
                        .color(t.muted),
                    );
                    changed |= ui
                        .checkbox(&mut self.voice.review_first, crate::tl_id!("voice.show_the_words_for_review_before_inserting_them_d313939a"))
                        .changed();
                });
                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new(crate::tl_id!("voice.check_your_setup_b61b421a")).strong());
                ui.horizontal(|ui| {
                    let testing = self.voice.mic_test.is_some();
                    if ui.add_enabled(!busy && !testing, egui::Button::new(crate::tl_id!("voice.test_microphone_49e76fed"))).clicked() {
                        action = Some("mic");
                    }
                    if let Some(test) = &self.voice.mic_test {
                        ui.label(crate::tl_id!("voice.say_something_9aa1505d"));
                        level_bar(ui, test.level, &t);
                    } else if let Some(result) = &self.voice.mic_result {
                        match result {
                            Ok(words) => ui.label(egui::RichText::new(words).color(t.ok)),
                            Err(words) => ui.add(egui::Label::new(egui::RichText::new(words).color(t.warn)).wrap()),
                        };
                    }
                });
                ui.horizontal(|ui| {
                    let testing = self.voice.recognizer_test.is_some();
                    if ui
                        .add_enabled(!busy && !testing && self.dictation_ready(), egui::Button::new(crate::tl_id!("voice.test_recognizer_f042c45f")))
                        .clicked()
                    {
                        action = Some("recognizer");
                    }
                    if testing {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(crate::tl_id!("voice.loading_the_model_45a4f448"));
                    } else if let Some(result) = &self.voice.recognizer_result {
                        match result {
                            Ok(words) => ui.label(egui::RichText::new(words).color(t.ok)),
                            Err(words) => ui.add(egui::Label::new(egui::RichText::new(words).color(t.warn)).wrap()),
                        };
                    }
                });
                if MIC_SETTINGS
                    && ui
                        .link(crate::i18n::format_message_id("voice.open_system_microphone_settings_11e3ebb5", &[("system", system_name())]))
                        .clicked()
                {
                    action = Some("settings");
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.dictation_ready(), primary(crate::tl_id!("voice.done_e9b450d1"), &t)).clicked() {
                        action = Some("close");
                    }
                    if !self.dictation_ready() {
                        ui.label(egui::RichText::new(crate::tl_id!("voice.choose_a_program_and_a_model_to_finish_82dc257d")).color(t.muted));
                    }
                });
            });
        if changed {
            self.save_voice_settings();
            self.voice.recognizer_result = None;
        }
        match action {
            Some("mic") => self.start_mic_test(),
            Some("recognizer") => self.start_recognizer_test(),
            Some("settings") => self.act_on_fix(Fix::MicSettings),
            Some("close") => open = false,
            _ => {}
        }
        if !open {
            self.voice.setup_open = false;
            self.voice.setup_reason = None;
            self.voice.mic_test = None;
            self.voice.recognizer_test = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{InvocationOrigin, State, VoiceUpdate};
    use notecraft_store::{Settings, testing::TempDir};

    fn app(format: PageFormat) -> (TempDir, NoteCraftApp) {
        let t = TempDir::new("voice-insertion");
        let settings = Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("privacy preference");
        settings.set("pressure_unavailable_never", "true").expect("test preference");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        session.run("notebook.create", &params(&[("name", "Voice"), ("format", format.id())])).expect("notebook");
        session
            .run(
                "page.save",
                &params(&[
                    ("path", "voice/new-section-1/untitled-page"),
                    (
                        "body",
                        if format == PageFormat::Html { "<p>Keep <b>bold</b> words</p>\n<p>Next</p>\n" } else { "Keep **bold** words\n\nNext\n" },
                    ),
                ]),
            )
            .expect("page");
        let mut app = NoteCraftApp::new(session);
        let path = t.path().join("notebooks/voice/new-section-1/untitled-page");
        app.open_page(&path);
        let ed = app.editor.as_mut().expect("editor");
        ed.caret = Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (0, 0), ..Default::default() });
        (t, app)
    }

    #[test]
    fn both_formats_insert_literal_unicode_and_support_one_undo() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let before = app.editor.as_ref().expect("editor").blocks.clone();
            let target = app.capture_voice_target().expect("target");
            app.apply_voice_text(&target, "Hola, mundo. مرحبا ").expect("insert");
            let after = app.editor.as_ref().expect("editor").blocks.clone();
            assert_eq!(after[1], before[1]);
            let notecraft_formats::Block::Paragraph(spans) = &after[0] else { panic!("paragraph") };
            assert!(spans.iter().any(|s| s.text.contains("Hola, mundo. مرحبا")));
            assert!(spans.iter().any(|s| s.text == "bold" && s.bold));
            app.save_now();
            let dir = app.editor.as_ref().expect("editor").dir.clone();
            let page = notecraft_store::page::load_page(&dir).expect("saved");
            let (persisted, _) = notecraft_formats::boxes::read(format, &page.body);
            assert_eq!(persisted, after);
            app.step_back(true);
            assert_eq!(app.editor.as_ref().expect("editor").blocks, before);
            app.step_back(false);
            assert_eq!(app.editor.as_ref().expect("editor").blocks, after);
            app.save_now();
            app.capture_voice_target().expect("a second recording can target the saved page");
        }
    }

    #[test]
    fn dictated_words_get_a_space_next_to_existing_words() {
        assert_eq!(super::spaced("Keep **bold** words", (4, 4), "hello there"), " hello there");
        assert_eq!(super::spaced("Keep **bold** words", (0, 0), "Hello."), "Hello. ");
        assert_eq!(super::spaced("Keep **bold** words", (11, 11), "er"), " er");
        assert_eq!(super::spaced("Keep **bold** words", (11, 11), "*"), "*");
        assert_eq!(super::spaced("end (", (5, 5), "aside"), "aside");
        assert_eq!(super::spaced("", (0, 0), "first words"), "first words");
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            app.editor.as_mut().expect("editor").caret.as_mut().expect("caret").selection = (4, 4);
            let target = app.capture_voice_target().expect("target");
            app.apply_voice_text(&target, "hello there").expect("insert");
            let line = pageedit::caret_text(&app.editor.as_ref().expect("editor").blocks[0]);
            assert_eq!(line, "Keep hello there **bold** words");
        }
    }

    #[test]
    fn the_caret_lands_after_inserted_punctuation_not_inside_the_next_word() {
        for format in PageFormat::ALL {
            for (words, before_caret) in [("$5", "prefix $5 "), ("*", "prefix * "), ("(a)", "prefix (a) ")] {
                let (_t, mut app) = app(format);
                let ed = app.editor.as_mut().expect("editor");
                ed.blocks =
                    vec![notecraft_formats::Block::Paragraph(vec![notecraft_formats::Span { text: "prefix suffix".into(), ..Default::default() }])];
                ed.rebuild_body();
                ed.caret = Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (7, 7), ..Default::default() });
                NoteCraftApp::mark_dirty_from_blocks(ed);
                app.save_now();
                let target = app.capture_voice_target().expect("target");
                app.apply_voice_text(&target, words).expect("insert");
                let ed = app.editor.as_ref().expect("editor");
                let caret = ed.caret.as_ref().expect("caret");
                let drawn: String = match &ed.blocks[0] {
                    notecraft_formats::Block::Paragraph(spans) => spans.iter().map(|s| s.text.as_str()).collect(),
                    other => panic!("paragraph expected, got {other:?}"),
                };
                assert_eq!(drawn, format!("{before_caret}suffix"), "{format:?}: the words read as typed");
                let written: String = caret.text.chars().take(caret.selection.0).collect();
                let rest: String = caret.text.chars().skip(caret.selection.0).collect();
                assert!(rest == "suffix", "{format:?} {words:?}: caret sits before {rest:?} in {:?} (after {written:?})", caret.text);
            }
        }
    }

    #[test]
    fn the_panel_warns_when_the_microphone_hears_nothing() {
        assert_eq!(super::input_hint(1.0, 0.0), None, "too early to tell");
        assert!(super::input_hint(2.5, 0.0).is_some(), "two seconds of digital silence");
        assert_eq!(super::input_hint(2.5, 0.2), None, "speech was heard");
        assert_eq!(super::meter(0.0), 0.0);
        assert_eq!(super::meter(1.0), 1.0);
        assert!((super::meter(0.01) - 1.0 / 3.0).abs() < 1e-4, "-40 dBFS fills a third");
    }

    #[test]
    fn changed_or_reopened_pages_reject_the_original_target() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        let target = app.capture_voice_target().expect("target");
        let dir = app.editor.as_ref().expect("editor").dir.clone();
        app.open_page(&dir);
        assert!(app.apply_voice_text(&target, "wrong place").is_err());
    }

    #[test]
    fn external_changes_reject_insertion_without_overwriting_the_file() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        let target = app.capture_voice_target().expect("target");
        let file = app.editor.as_ref().expect("editor").dir.join("index.md");
        std::fs::write(&file, "external words").expect("external edit");
        assert!(app.apply_voice_text(&target, "do not write").is_err());
        assert_eq!(std::fs::read_to_string(file).expect("read"), "external words");
        assert!(app.capture_voice_target().is_err(), "retargeting must not authorize writing a stale editor over an external change");
    }

    #[test]
    fn insertion_cannot_approve_words_different_from_the_visible_draft() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        let ticket = app.capture_voice_target().expect("target");
        app.session.run_with_origin("voice.start", &params(&[("target", &ticket.id)]), InvocationOrigin::LocalUi).expect("session");
        app.session.voice_update(VoiceUpdate::Ready(1)).expect("ready");
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "accepted draft", true))).expect("draft");
        app.voice.target = Some(ticket);
        app.voice.review = "different unaccepted draft".into();
        app.voice.review_revision = 1;
        assert!(app.commit_voice().is_err());
        assert_eq!(app.session.voice().state(), State::Review);
        app.voice.review = "accepted draft".into();
        assert!(app.stop_voice().is_ok(), "releasing a hold after finalization is harmless");
        app.commit_voice().expect("insert approved words");
        let once = app.editor.as_ref().expect("editor").body.clone();
        assert!(app.commit_voice().is_err());
        assert_eq!(app.editor.as_ref().expect("editor").body, once);
    }

    #[cfg(unix)]
    #[test]
    fn linked_cache_cannot_redirect_audio_into_a_notebook() {
        let (t, mut app) = app(PageFormat::Cmgfm);
        let inside = t.path().join("notebooks/voice/audio-cache");
        std::fs::create_dir(&inside).expect("cache target");
        std::os::unix::fs::symlink(&inside, t.path().join("voice-cache")).expect("cache link");
        assert!(app.start_voice().is_err(), "linked cache must fail before starting a runtime");
        assert_eq!(app.session.voice().state(), State::Failed);
        assert_eq!(std::fs::read_dir(&inside).expect("cache").count(), 0);
    }

    /// Words that look like markup either go in exactly as said, or are refused with the
    /// draft, the page and the file untouched. Which one depends on what the page format's
    /// serializer can keep literal, so both outcomes are checked rather than assumed.
    #[test]
    fn literal_punctuation_is_kept_exactly_or_refused_with_the_draft_and_page_unchanged() {
        for format in PageFormat::ALL {
            for words in ["*literal*", "[a](b)", r"C:\notes\page", "#important buy milk", "----"] {
                let (_t, mut app) = app(format);
                app.editor.as_mut().expect("editor").caret = None;
                let ticket = app.capture_voice_target().expect("target");
                let before = app.editor.as_ref().expect("editor").body.clone();
                let disk_before = std::fs::read(super::page_file(app.editor.as_ref().expect("editor"))).expect("original page");
                app.session.run_with_origin("voice.start", &params(&[("target", &ticket.id)]), InvocationOrigin::LocalUi).expect("start");
                app.session.voice_update(VoiceUpdate::Ready(1)).expect("ready");
                app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
                app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, words, true))).expect("draft");
                app.voice.target = Some(ticket);
                app.voice.review = words.into();
                app.voice.review_revision = 1;
                if app.commit_voice().is_err() {
                    assert_eq!(app.session.voice().state(), State::Review);
                    assert_eq!(app.session.voice().draft(), words);
                    assert_eq!(app.voice.review, words);
                    assert_eq!(app.editor.as_ref().expect("editor").body, before);
                    assert_eq!(std::fs::read(super::page_file(app.editor.as_ref().expect("editor"))).expect("page"), disk_before);
                } else {
                    app.save_now();
                    let ed = app.editor.as_ref().expect("editor");
                    let last = ed.blocks.last().expect("inserted paragraph");
                    assert_eq!(pageedit::block_text(last), words, "{format:?} {words:?} must go in exactly as said");
                    let page = notecraft_store::page::load_page(&ed.dir).expect("saved");
                    let (persisted, _) = notecraft_formats::boxes::read(format, &page.body);
                    assert_eq!(persisted, ed.blocks, "{format:?} {words:?} must reopen as inserted");
                }
            }
        }
    }

    #[test]
    fn rejected_edit_suppresses_pending_insert_and_keeps_rejected_words() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        let ticket = app.capture_voice_target().expect("target");
        app.session.run_with_origin("voice.start", &params(&[("target", &ticket.id)]), InvocationOrigin::LocalUi).expect("start");
        app.session.voice_update(VoiceUpdate::Ready(1)).expect("ready");
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "old accepted words", true))).expect("draft");
        app.voice.target = Some(ticket);
        app.voice.review = "é".repeat(craft_voice_core::MAX_TRANSCRIPT_BYTES / 2 + 1);
        let rejected = app.voice.review.clone();
        app.voice.review_revision = 1;
        let before = app.editor.as_ref().expect("editor").body.clone();
        let mut action = Some("commit");
        app.submit_voice_edit(&mut action);
        assert_eq!(action, None, "a rejected edit cannot approve the prior draft in this frame");
        assert_eq!(app.voice.review, rejected);
        assert_eq!(app.session.voice().state(), State::Review);
        assert_eq!(app.editor.as_ref().expect("editor").body, before);
        assert!(app.commit_voice().is_err());
    }

    #[test]
    fn a_caret_in_code_or_maths_sends_the_words_to_the_end_of_the_page() {
        for format in PageFormat::ALL {
            for code in [true, false] {
                let (_t, mut app) = app(format);
                let ed = app.editor.as_mut().expect("editor");
                ed.blocks = vec![notecraft_formats::Block::Paragraph(vec![
                    notecraft_formats::Span { text: "before ".into(), ..Default::default() },
                    notecraft_formats::Span { text: "abc".into(), code, math: !code, ..Default::default() },
                    notecraft_formats::Span { text: " after".into(), ..Default::default() },
                ])];
                ed.rebuild_body();
                ed.caret = Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (10, 10), ..Default::default() });
                NoteCraftApp::mark_dirty_from_blocks(ed);
                app.save_now();
                let line = app.editor.as_ref().expect("editor").blocks[0].clone();
                let target = app.capture_voice_target().expect("the end of the page takes the words");
                assert!(matches!(target.anchor, super::Anchor::End));
                app.apply_voice_text(&target, "literal words.").expect("insert");
                let ed = app.editor.as_ref().expect("editor");
                assert_eq!(ed.blocks[0], line, "{format:?}: the code or maths line is untouched");
                assert_eq!(ed.blocks.len(), 2);
                assert_eq!(pageedit::block_text(&ed.blocks[1]), "literal words.");
            }
        }
    }

    #[test]
    fn rejected_insertion_does_not_add_a_history_step() {
        // "   " is always refused. The others are refused only where the page format cannot
        // keep them literal; when one goes in instead, it must be exactly one Undo step.
        for words in ["   ", "*literal*", "[a](b)", r"C:\notes\page", "----"] {
            let (_t, mut app) = app(PageFormat::Cmgfm);
            let target = app.capture_voice_target().expect("target");
            let before = app.editor.as_ref().expect("editor").blocks.clone();
            let refused = app.apply_voice_text(&target, words).is_err();
            assert!(refused || !words.trim().is_empty(), "blank words are always refused");
            app.step_back(true);
            assert_eq!(app.editor.as_ref().expect("editor").blocks, before, "{words:?}, refused: {refused}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn linked_cache_in_an_externally_opened_notebook_is_rejected() {
        let (t, mut app) = app(PageFormat::Cmgfm);
        let mut other = Session::new(&t.path().join("external-root"), app.session.settings.clone()).expect("other session");
        other.run("notebook.create", &params(&[("name", "Elsewhere")])).expect("notebook");
        let notebook = t.path().join("external-root/elsewhere");
        app.session.open_notebook(&notebook).expect("open elsewhere");
        let inside = notebook.join("voice-cache");
        std::fs::create_dir(&inside).expect("cache target");
        std::os::unix::fs::symlink(&inside, t.path().join("voice-cache")).expect("link");
        assert!(app.start_voice().is_err());
        assert!(std::fs::read_dir(inside).expect("cache").next().is_none());
    }

    #[test]
    fn setup_errors_do_not_leave_a_session_permanently_preparing() {
        let (t, mut app) = app(PageFormat::Cmgfm);
        app.session.settings = Settings::at(t.path().join("absent/settings.toml"));
        assert!(app.start_voice().is_err());
        assert_eq!(app.session.voice().state(), State::Failed);
    }

    #[test]
    fn single_block_selection_preserves_boxes_and_rejects_later_edits() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let ed = app.editor.as_mut().expect("editor");
            ed.caret.as_mut().expect("caret").selection = (0, 4);
            ed.arranged = true;
            ed.boxes = vec![notecraft_model::Container::new(14.0, 22.0, 420.0, 2)];
            ed.rebuild_body();
            NoteCraftApp::mark_dirty_from_blocks(ed);
            let target = app.capture_voice_target().expect("target");
            let before_boxes = app.editor.as_ref().expect("editor").boxes.clone();
            app.apply_voice_text(&target, "Replace").expect("replace selection");
            let ed = app.editor.as_mut().expect("editor");
            assert!(pageedit::caret_text(&ed.blocks[0]).starts_with("Replace **bold** words"));
            assert_eq!(ed.boxes, before_boxes);
            let after = ed.body.clone();
            assert!(app.apply_voice_text(&target, "again").is_err());
            assert_eq!(app.editor.as_ref().expect("editor").body, after);
        }
    }

    fn harness(app: NoteCraftApp, mode: Mode) -> egui_kittest::Harness<'static, NoteCraftApp> {
        let mut app = app;
        app.set_theme(Theme { mode, accent: Accent::Default });
        // The page's own editor has no accessible name yet, which is why the app's UI tests turn
        // the harness check off; the dictation fields are checked by name below instead.
        egui_kittest::Harness::builder()
            .with_size(vec2(1200.0, 800.0))
            .with_accessibility_check(false)
            .wgpu()
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app)
    }

    fn review_picture(h: &mut egui_kittest::Harness<'static, NoteCraftApp>, name: &str) {
        let picture = h.render().expect("render");
        if let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("visual folder");
            picture.save(dir.join(format!("{name}.png"))).expect("review image");
        }
    }

    #[test]
    fn the_mic_button_opens_setup_until_a_recognizer_is_chosen() {
        use egui_kittest::kittest::Queryable;
        for (name, mode) in [("light", Mode::Light), ("dark", Mode::Dark)] {
            let (_t, app) = app(PageFormat::Cmgfm);
            let mut h = harness(app, mode);
            for _ in 0..4 {
                h.step();
            }
            h.get_by_label("Dictate").click();
            for _ in 0..4 {
                h.step();
            }
            assert!(h.state().voice.setup_open, "no recognizer yet: the button opens setup");
            assert_eq!(h.state().session.voice().state(), State::Idle, "and does not start recording");
            for field in ["Recognizer program (whisper-cli)", "Speech model"] {
                let named = h.query_all_by_role_and_label(egui::accesskit::Role::TextInput, field).count();
                assert_eq!(named, 1, "the {field:?} field is named for screen readers");
            }
            let _ = h.get_by_label("Test microphone");
            review_picture(&mut h, &format!("dictation-setup-{name}"));
        }
    }

    /// Replays transcript data through the real engine to inspect UI states.
    /// This does not exercise microphone capture or speech recognition.
    fn listening(app: &mut NoteCraftApp, words: &str) {
        let target = app.capture_voice_target().expect("target");
        app.session.run_with_origin("voice.start", &params(&[("target", &target.id)]), InvocationOrigin::LocalUi).expect("start");
        app.voice.target = Some(target);
        app.session.voice_update(VoiceUpdate::Ready(1)).expect("ready");
        app.voice.seconds = 7.0;
        app.voice.level = 0.3;
        app.voice.loudest = 0.3;
        if !words.is_empty() {
            app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, words, false))).expect("preview");
        }
    }

    #[test]
    fn the_recording_bar_and_preview_show_at_the_insertion_point() {
        use egui_kittest::kittest::Queryable;
        for (name, mode) in [("light", Mode::Light), ("dark", Mode::Dark)] {
            let (_t, app) = app(PageFormat::Cmgfm);

            let mut h = harness(app, mode);
            for _ in 0..4 {
                h.step();
            }
            // Click into the first line, as a person would, once the page is up.
            let ed = h.state_mut().editor.as_mut().expect("editor");
            ed.caret =
                Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (0, 0), focus: true, ..Default::default() });
            for _ in 0..2 {
                h.step();
            }
            listening(h.state_mut(), "Remember to call the venue about the date");
            for _ in 0..4 {
                h.step();
            }
            review_picture(&mut h, &format!("dictation-listening-{name}"));
            let _ = h.get_by_label("Listening");
            let _ = h.get_by_label("Done");
            let _ = h.get_by_label("Finish dictating");
            let _ = h.get_by_label("Preview, going at your cursor");

            h.state_mut().session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
            h.state_mut()
                .session
                .voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 2, "Remember to call the venue about the date.", true)))
                .expect("final");
            for _ in 0..4 {
                h.step();
            }
            let _ = h.get_by_label("Ready to insert at your cursor");
            review_picture(&mut h, &format!("dictation-review-{name}"));
            h.get_by_label("Insert here").click();
            for _ in 0..4 {
                h.step();
            }
            assert_eq!(h.state().session.voice().state(), State::Complete);
            let body = h.state().editor.as_ref().expect("editor").body.clone();
            // The line took the keyboard with the caret at its end, so that is where the words go,
            // with the space typing would have put before them.
            assert!(body.contains("Keep **bold** words Remember to call the venue about the date.\n"), "inserted at the caret:\n{body}");
        }
    }

    #[test]
    fn enter_finishes_and_escape_cancels_without_touching_the_page() {
        let (_t, app) = app(PageFormat::Cmgfm);

        let mut h = harness(app, Mode::Light);
        h.step();
        listening(h.state_mut(), "");
        let before = h.state().editor.as_ref().expect("editor").body.clone();
        h.key_press(egui::Key::Enter);
        h.step();
        assert_eq!(h.state().session.voice().state(), State::Finalizing, "Enter finishes");
        assert_eq!(h.state().editor.as_ref().expect("editor").body, before, "and does not split the paragraph");
        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(h.state().session.voice().state(), State::Cancelled, "Esc cancels");
        assert_eq!(h.state().editor.as_ref().expect("editor").body, before);
    }

    #[test]
    fn with_no_cursor_the_words_become_a_paragraph_at_the_end() {
        for format in PageFormat::ALL {
            for boxed in [false, true] {
                let (_t, mut app) = app(format);
                let ed = app.editor.as_mut().expect("editor");
                ed.caret = None;
                if boxed {
                    ed.arranged = true;
                    ed.boxes = vec![notecraft_model::Container::new(10.0, 10.0, 300.0, 1), notecraft_model::Container::new(10.0, 200.0, 300.0, 1)];
                    ed.rebuild_body();
                    NoteCraftApp::mark_dirty_from_blocks(ed);
                }
                let before = app.editor.as_ref().expect("editor").blocks.clone();
                let target = app.capture_voice_target().expect("target");
                assert!(matches!(target.anchor, super::Anchor::End));
                app.apply_voice_text(&target, "A new thought.").expect("insert");
                let ed = app.editor.as_ref().expect("editor");
                assert_eq!(ed.blocks[..before.len()], before[..], "{format:?}: nothing before it changes");
                assert_eq!(pageedit::block_text(&ed.blocks[before.len()]), "A new thought.");
                assert_eq!(ed.caret.as_ref().map(|c| c.block), Some(before.len()), "the caret follows the words");
                if boxed {
                    assert_eq!(ed.boxes.iter().map(|c| c.blocks).collect::<Vec<_>>(), vec![1, 2], "{format:?}: the last box grows");
                }
                app.save_now();
                let page = notecraft_store::page::load_page(&app.editor.as_ref().expect("editor").dir).expect("saved");
                let (blocks, boxes) = notecraft_formats::boxes::read(format, &page.body);
                assert_eq!(pageedit::block_text(&blocks[before.len()]), "A new thought.", "{format:?} boxed={boxed}");
                assert_eq!(boxes.is_some(), boxed);
            }
        }
    }

    #[test]
    fn several_lines_become_several_paragraphs_and_one_undo_takes_them_back() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let before = app.editor.as_ref().expect("editor").blocks.clone();
            let caret = app.editor.as_mut().expect("editor").caret.as_mut().expect("caret");
            let end = caret.text.chars().count();
            caret.selection = (end, end);
            let target = app.capture_voice_target().expect("target");
            app.apply_voice_text(&target, "First line\n\nSecond line\nThird line").expect("insert");
            let ed = app.editor.as_ref().expect("editor");
            assert!(pageedit::block_text(&ed.blocks[0]).ends_with(" First line"), "{format:?}: the first line follows the existing words");
            assert_eq!(pageedit::block_text(&ed.blocks[1]), "Second line");
            assert_eq!(pageedit::block_text(&ed.blocks[2]), "Third line");
            assert_eq!(ed.blocks[3], before[1], "the next paragraph moves down");
            assert_eq!(ed.caret.as_ref().map(|c| (c.block, c.selection.0)), Some((2, "Third line".len())));
            app.step_back(true);
            assert_eq!(app.editor.as_ref().expect("editor").blocks, before);
        }
    }

    #[test]
    fn a_page_changed_during_review_takes_the_words_at_the_cursor_now() {
        let (_t, mut app) = app(PageFormat::Cmgfm);

        listening(&mut app, "");
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "late words", true))).expect("final");
        app.poll_voice(&egui::Context::default());
        // Typed into the page while the words waited, then moved the cursor to the next line.
        let ed = app.editor.as_mut().expect("editor");
        ed.caret = Some(pageedit::Caret { block: 0, text: "Typed meanwhile".into(), selection: (15, 15), ..Default::default() });
        pageedit::set_caret_text(&mut ed.blocks[0], "Typed meanwhile");
        ed.rebuild_body();
        NoteCraftApp::mark_dirty_from_blocks(ed);
        app.save_now();
        let ed = app.editor.as_mut().expect("editor");
        ed.caret = Some(pageedit::Caret { block: 1, text: pageedit::caret_text(&ed.blocks[1]), selection: (4, 4), ..Default::default() });
        app.commit_voice().expect("inserted where the cursor is now");
        let ed = app.editor.as_ref().expect("editor");
        assert_eq!(pageedit::block_text(&ed.blocks[0]), "Typed meanwhile", "what was typed is kept");
        assert_eq!(pageedit::block_text(&ed.blocks[1]), "Next late words");
        assert_eq!(app.session.voice().state(), State::Complete);
    }

    #[test]
    fn with_review_off_finished_words_go_straight_in() {
        let (_t, mut app) = app(PageFormat::Cmgfm);

        app.voice.review_first = false;
        listening(&mut app, "");
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "Straight in", true))).expect("final");
        app.poll_voice(&egui::Context::default());
        assert_eq!(app.session.voice().state(), State::Complete);
        assert!(pageedit::block_text(&app.editor.as_ref().expect("editor").blocks[0]).starts_with("Straight in Keep"));
    }

    /// Opens a second page in the same section and returns its folder.
    fn open_second_page(t: &TempDir, app: &mut NoteCraftApp) -> PathBuf {
        let created = app
            .session
            .run("node.create", &params(&[("parent", "voice/new-section-1"), ("kind", "page"), ("title", "Second page"), ("format", "cmgfm")]))
            .expect("second page");
        let Output::Path(path) = created else { panic!("a page path") };
        let path = if path.is_absolute() { path } else { t.path().join("notebooks").join(path) };
        let rel = path.strip_prefix(t.path().join("notebooks")).expect("in notebooks").to_string_lossy().replace('\\', "/");
        app.session.run("page.save", &params(&[("path", &rel), ("body", "Other page words\n")])).expect("second page words");
        app.open_page(&path);
        path
    }

    #[test]
    fn finished_words_never_land_on_another_page_without_asking() {
        for review_first in [false, true] {
            let (t, mut app) = app(PageFormat::Cmgfm);

            app.voice.review_first = review_first;
            listening(&mut app, "");
            let first = app.editor.as_ref().expect("editor").dir.clone();
            let first_body = notecraft_store::page::load_page(&first).expect("first").body;
            app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
            // The person opens another page before the final words arrive.
            let second = open_second_page(&t, &mut app);
            app.session
                .voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "meant for the first page", true)))
                .expect("final");
            app.poll_voice(&egui::Context::default());
            if review_first {
                assert!(app.commit_voice().is_err(), "Insert on another page asks first");
            }
            assert_eq!(app.session.voice().state(), State::Review, "the words wait (review_first={review_first})");
            assert_eq!(notecraft_store::page::load_page(&second).expect("second").body.trim(), "Other page words");
            assert_eq!(notecraft_store::page::load_page(&first).expect("first").body, first_body);
            assert!(app.target_elsewhere().is_some_and(|title| !title.is_empty()), "the preview names the page the words were for");
            app.commit_on_this_page().expect("an explicit choice moves them");
            app.save_now();
            assert!(notecraft_store::page::load_page(&second).expect("second").body.contains("meant for the first page"));
        }
    }

    #[test]
    fn losing_focus_ends_a_held_recording() {
        let (_t, app) = app(PageFormat::Cmgfm);

        let mut h = harness(app, Mode::Light);
        h.step();
        listening(h.state_mut(), "");
        h.state_mut().voice.pressed_at = Some(Instant::now() - std::time::Duration::from_millis(100));
        h.input_mut().focused = false;
        h.input_mut().events.push(egui::Event::WindowFocused(false));
        h.step();
        assert_eq!(h.state().session.voice().state(), State::Finalizing, "the window lost focus with the key held");
    }

    #[test]
    fn a_released_key_ends_a_hold_even_when_the_release_event_was_missed() {
        let (_t, app) = app(PageFormat::Cmgfm);

        let mut h = harness(app, Mode::Light);
        h.step();
        listening(h.state_mut(), "");
        // Held for a second, and the key is up now, but no release event ever arrived.
        h.state_mut().voice.pressed_at = Some(Instant::now() - std::time::Duration::from_secs(1));
        h.step();
        assert_eq!(h.state().session.voice().state(), State::Finalizing);
        // A tap leaves recording on, as before.
        let (_t, app) = self::app(PageFormat::Cmgfm);

        let mut h = harness(app, Mode::Light);
        h.step();
        listening(h.state_mut(), "");
        h.state_mut().voice.pressed_at = Some(Instant::now() - std::time::Duration::from_millis(100));
        h.step();
        assert_eq!(h.state().session.voice().state(), State::Recording, "a tap toggles: recording goes on");
    }

    #[test]
    fn a_failed_recording_is_never_inserted_on_its_own() {
        let (_t, mut app) = app(PageFormat::Cmgfm);

        app.voice.review_first = false;
        listening(&mut app, "half a sentence");
        let before = app.editor.as_ref().expect("editor").body.clone();
        app.voice.error = Some("microphone disconnected".into());
        app.session.voice_update(VoiceUpdate::Failed(1, "microphone disconnected".into())).expect("failed");
        app.poll_voice(&egui::Context::default());
        assert_eq!(app.session.voice().state(), State::Review, "the partial words stay for review");
        assert_eq!(app.session.voice().draft(), "half a sentence");
        assert_eq!(app.editor.as_ref().expect("editor").body, before, "nothing was inserted");
    }

    #[test]
    fn caret_insertion_rejects_new_tags_without_invisible_characters() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let ticket = app.capture_voice_target().expect("target");
            let before = app.editor.as_ref().expect("editor").blocks.clone();
            assert!(app.apply_voice_text(&ticket, "#idea").is_err());
            assert_eq!(app.editor.as_ref().expect("editor").blocks, before);
        }
    }

    #[test]
    fn errors_are_explained_with_something_to_do() {
        use super::{Fix, explain};
        let (words, fix) = explain("The microphone opened but sent no audio for 5 seconds. Check …");
        assert!(words.contains("can't hear your microphone"));
        assert_eq!(fix, if super::MIC_SETTINGS { Fix::MicSettings } else { Fix::Setup });
        assert_eq!(explain("choose an existing absolute path to whisper-cli").1, Fix::Setup);
        assert_eq!(explain("choose an existing local Whisper model file").1, Fix::Setup);
        assert_eq!(explain("recognizer timed out").1, Fix::Setup);
        assert_eq!(explain("Return to the page view before dictating.").1, Fix::PageView);
        assert_eq!(explain("something unforeseen"), ("something unforeseen".to_string(), Fix::None));
    }

    #[test]
    fn successful_insertion_clears_an_earlier_ui_error() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        listening(&mut app, "");
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "Reviewed words", true))).expect("final");
        app.poll_voice(&egui::Context::default());
        app.voice.error = Some("earlier insertion failure".into());
        app.commit_voice().expect("insert");
        assert!(app.voice.error.is_none(), "completed dictation must not show a stale error bar");
    }

    #[test]
    fn setup_and_review_messages_use_catalogs_with_named_placeholders() {
        for text in ["Dictation setup", "Dictated words", "Test microphone", "No speech was recognized."] {
            assert_eq!(crate::i18n::translate_for("en-XA", text), format!("$$${text}@@@"));
            assert_eq!(crate::i18n::pseudo_unmarked(&crate::i18n::translate_for("ar-XB", text)), text);
        }
        let message = crate::i18n::format_message_for("en-XA", "Ready to insert {place}", &[("place", "PLACE")]);
        assert_eq!(message, "$$$Ready to insert PLACE@@@");
        for (_, id) in super::LANGUAGES {
            assert!(crate::i18n::english_text(id).is_some(), "speech language label {id} has an English source");
        }
    }

    #[test]
    fn idle_shortcut_does_not_open_setup_behind_another_dialog() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        app.voice.setup_open = true;
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            events: vec![egui::Event::Key { key: Key::F8, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
            ..Default::default()
        });
        app.dictation_keys(&ctx);
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        assert_eq!(app.session.voice().state(), State::Idle);
        assert!(app.voice.pressed_at.is_none());
        assert!(app.voice.runtime.is_none());
    }

    #[test]
    fn multiline_insertion_with_trailing_content_requires_explicit_append() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let ed = app.editor.as_mut().expect("editor");
            pageedit::set_caret_text(&mut ed.blocks[0], "before **AFTER**");
            ed.caret = Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (7, 7), ..Default::default() });
            ed.rebuild_body();
            NoteCraftApp::mark_dirty_from_blocks(ed);
            let target = app.capture_voice_target().expect("target");
            let before = app.editor.as_ref().expect("editor").blocks.clone();
            assert!(app.apply_voice_text(&target, "one\ntwo").is_err(), "existing suffix must not appear between transcript lines");
            assert_eq!(app.editor.as_ref().expect("editor").blocks, before);
        }
    }

    #[test]
    fn automatic_insertion_keeps_review_when_the_page_or_selection_changed() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        app.voice.review_first = false;
        listening(&mut app, "");
        let ed = app.editor.as_mut().expect("editor");
        pageedit::set_caret_text(&mut ed.blocks[0], "Changed existing words");
        ed.caret = Some(pageedit::Caret { block: 0, text: pageedit::caret_text(&ed.blocks[0]), selection: (0, 7), ..Default::default() });
        ed.rebuild_body();
        NoteCraftApp::mark_dirty_from_blocks(ed);
        let before = ed.body.clone();
        app.session.voice_update(VoiceUpdate::Stopped(1)).expect("stop");
        app.session.voice_update(VoiceUpdate::Transcript(craft_voice_core::Transcript::new(1, 1, "final transcript", true))).expect("final");
        app.poll_voice(&egui::Context::default());
        assert_eq!(app.session.voice().state(), State::Review, "changed targets need explicit Insert");
        assert_eq!(app.editor.as_ref().expect("editor").body, before, "new selections must not be silently replaced");
        assert_eq!(app.voice.review, "final transcript");
    }

    #[test]
    fn moving_the_caret_cannot_move_the_preview_away_from_its_insertion_ticket() {
        let (_t, mut app) = app(PageFormat::Cmgfm);
        let ticket = app.capture_voice_target().expect("target");
        app.voice.target = Some(ticket);
        app.editor.as_mut().expect("editor").caret.as_mut().expect("caret").selection = (4, 4);
        let ctx = egui::Context::default();
        let spot = egui::Rect::from_min_size(egui::pos2(50.0, 50.0), vec2(1.0, 20.0));
        let line = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), vec2(300.0, 40.0));
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|data| data.insert_temp(pageedit::caret_anchor_id(), (spot, line, pass, 0_usize)));
        assert!(app.preview_anchor(&ctx).is_none(), "a moved caret is not the saved insertion point");
    }

    #[test]
    fn multiline_insertion_cannot_reassign_indexed_layout_to_different_blocks() {
        for format in PageFormat::ALL {
            let (_t, mut app) = app(format);
            let ed = app.editor.as_mut().expect("editor");
            ed.blocks.push(notecraft_formats::Block::Table {
                head: vec![vec![notecraft_formats::Span { text: "Column".into(), ..Default::default() }]],
                rows: vec![],
            });
            ed.columns.set(2, 0, 244.0, &[160.0]);
            ed.justified.set(1, Some(notecraft_model::justify::Strength::High));
            ed.rebuild_body();
            NoteCraftApp::mark_dirty_from_blocks(ed);
            NoteCraftApp::remember_step(ed);
            let dir = ed.dir.clone();
            notecraft_store::page::set_table_columns(&dir, &ed.columns).expect("save widths");
            notecraft_store::page::set_justified(&dir, &ed.justified).expect("save justification");
            app.save_now();
            let ed = app.editor.as_ref().expect("editor");
            let before = ed.body.clone();
            let columns = ed.columns.clone();
            let justified = ed.justified.clone();
            let caret = app.editor.as_mut().expect("editor").caret.as_mut().expect("caret");
            let end = caret.text.chars().count();
            caret.selection = (end, end);
            let ticket = app.capture_voice_target().expect("target");
            assert!(app.apply_voice_text(&ticket, "First line\nSecond line").is_err(), "indexed layout must not migrate to different content");
            let ed = app.editor.as_ref().expect("editor");
            assert_eq!(ed.body, before);
            assert_eq!(ed.columns, columns);
            assert_eq!(ed.justified, justified);
            assert_eq!(notecraft_store::page::table_columns(&dir).expect("widths"), columns);
            assert_eq!(notecraft_store::page::justified(&dir).expect("justification"), justified);
            app.editor.as_mut().expect("editor").caret = None;
            let end = app.capture_voice_target().expect("append target");
            app.apply_voice_text(&end, "First appended line\nSecond appended line").expect("end append preserves all indexes");
            let after = app.editor.as_ref().expect("editor").body.clone();
            app.save_now();
            app.step_back(true);
            assert_eq!(app.editor.as_ref().expect("editor").body, before);
            app.step_back(false);
            assert_eq!(app.editor.as_ref().expect("editor").body, after);
            app.save_now();
            assert_eq!(notecraft_store::page::table_columns(&dir).expect("widths"), columns);
            assert_eq!(notecraft_store::page::justified(&dir).expect("justification"), justified);
        }
    }

    #[test]
    fn a_tap_toggles_and_a_hold_talks() {
        assert!(!super::release_finishes(std::time::Duration::from_millis(150)), "a tap leaves recording on");
        assert!(super::release_finishes(std::time::Duration::from_millis(900)), "letting go after a hold finishes");
        assert_eq!(super::clock(0.4), "0:00");
        assert_eq!(super::clock(75.2), "1:15");
    }
}
