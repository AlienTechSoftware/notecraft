//! Stable ids for nodes. Not a standard ULID or UUID: a timestamp plus a counter,
//! unique on one machine. Replace with a ULID before notebooks are merged across devices.

use std::sync::atomic::{AtomicU32, Ordering};
// std's own SystemTime panics on wasm32-unknown-unknown, which has no clock; web_time is
// the same type everywhere else and Date.now() there. See the workspace Cargo.toml.
use web_time::{SystemTime, UNIX_EPOCH};

pub fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| u64::try_from(d.as_nanos()).unwrap_or(0)).unwrap_or(0);
    format!("{nanos:016x}{:04x}", COUNTER.fetch_add(1, Ordering::Relaxed) & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 20);
    }
}
