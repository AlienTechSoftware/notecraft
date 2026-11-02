# First Launch: Remove Welcome Notebook, Add Logging Consent Dialog

- Date: 2026-10-31
- Status: Implemented and verified in the current baseline
- Branch: `feat/v0.0.8`

---

## Problem

**Current behavior:**
- App tries to auto-deploy "Welcome notebook" at startup
- This fails if folder doesn't exist or is read-only
- Shows ugly error message to user (raw OS errors)
- User confused about what's happening

**Reality:**
- This is NOT an app issue
- This is a **setup/deployment** issue
- App should NOT try to deploy files at startup

**What users actually need:**
- Logging/telemetry consent (first launch only)
- Quick tutorial (optional, user-initiated)
- Not forced deployment attempts

---

## Solution

### Part 1: Remove Welcome Notebook Auto-Deployment

**File:** `apps/notecraft/src/main.rs` lines 145-161

**Change:**
```rust
// REMOVE THIS ENTIRE BLOCK:
match session.root().join("welcome-to-notecraft").try_exists() {
    Ok(false) => { ... deploy notebook ... }
    Ok(_) => {}
    Err(e) => { ... show error ... }
}
```

**Reason:** Setup tasks should NOT happen in the running app.

### Part 2: Add First-Launch Logging Dialog

Instead of auto-deploying, show a dialog **once per installation:**

```rust
fn show_first_launch_dialog() -> bool {
    // Check if "logging-consent" has been shown before
    let shown = session.settings.get("logging-consent-shown").is_some();
    if shown {
        return false;  // Already shown, don't show again
    }
    
    // Show dialog
    let response = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("Welcome to NoteCraft")
        .set_description(
            "Help improve NoteCraft by sending anonymous usage data? \
             This includes crash reports and feature usage, never document content. \
             You can change this anytime in Settings."
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    
    // Store that we asked
    session.settings.set("logging-consent-shown", "true");
    session.save_settings();
    
    response == rfd::MessageDialogResult::Yes
}
```

**Call this:** In `main()` before drawing UI, if it's the first launch.

---

## What NOT to Do

❌ DO NOT auto-deploy files
❌ DO NOT show OS error messages  
❌ DO NOT check for optional notebooks
❌ DO NOT create directories automatically

---

## What TO Do

✅ Show logging consent dialog (first launch)
✅ Let user create/download notebooks manually (File menu)
✅ Provide welcome tour (interactive, not forced)
✅ Log technical details for developers, not users

---

## Test Scenario

### First Launch (Fresh Installation)
1. Start app for first time
2. See friendly dialog: "Help improve NoteCraft by sending data?"
   - [Yes]  [No]
3. App remembers choice
4. No errors, no failed deployments

### Second Launch
1. Start app again
2. Dialog does NOT appear (already shown)
3. App runs normally

### If Logging Disabled
1. Settings > Privacy
2. Toggle "Send usage data"
3. Works immediately (no restart needed)

---

## Files to Modify

- `apps/notecraft/src/main.rs` - Remove lines 145-161, add first-launch dialog
- `crates/ui-egui/src/settings.rs` - Track "logging-consent-shown" flag
- Tests: Verify first-launch behavior

---

## Acceptance Criteria

- [x] No Welcome notebook deployment attempted
- [x] No OS errors shown during startup
- [x] Logging-level dialog appears once per installation
- [x] Dialog can be dismissed and choice is remembered
- [x] User can change the level later in Settings
- [x] No network calls are made by this local-only diagnostic path

## Current disposition

The startup deployment path was removed. First-use state is covered by the UI
tests, and the current Options/About surfaces expose the local logging level and
privacy behavior. Keep this document as the implementation record; future
changes to consent wording or collection policy belong to `nc-prod`.
