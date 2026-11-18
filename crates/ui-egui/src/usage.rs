//! Usage events from the interface, such as a font chosen or a font added. The app installs a sink
//! once at start; the sink writes each event to the local telemetry session and the local log. With
//! no sink installed (tests, the demo windows) events are dropped. Nothing is uploaded.
//!
//! Fields must never hold a file path or the text of a note. Family names are allowed.

use std::sync::OnceLock;

/// How much an event matters. The app keeps an event when its telemetry level allows this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Usage,
    Warning,
    Error,
}

/// Receives each event: its level, its name, and its fields as (name, value) pairs.
pub type Sink = Box<dyn Fn(Level, &str, &[(&str, &str)]) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();

/// Installs the sink. Only the first call takes effect.
pub fn install(sink: Sink) {
    let _ = SINK.set(sink);
}

/// Records an event. Cheap when no sink is installed.
pub fn record(level: Level, event: &str, fields: &[(&str, &str)]) {
    if let Some(sink) = SINK.get() {
        sink(level, event, fields);
    }
}
