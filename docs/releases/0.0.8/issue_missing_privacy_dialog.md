# Missing PrivacyWelcome Dialog on Startup

- Date: 2026-10-31
- Status: Fixed and regression-tested
- Branch: `feat/v0.0.8`
- Severity: High (user doesn't know logging level)

---

## Problem

The PrivacyWelcome dialog that shows the logging level to the user **does not appear** on startup.

**Expected behavior:**
- First launch: Show PrivacyWelcome dialog with logging level (debug/usage/none)
- User sees: "Logging is set to: DEBUG"
- Dialog gets dismissed, setting `privacy_welcome_seen = true`

**Actual behavior:**
- App starts
- No dialog shown
- User doesn't know logging level is set to debug

---

## Code Location

The dialog exists in code:
- `Dialog::PrivacyWelcome { level }` enum variant (app.rs line 441)
- Shown when `privacy_welcome = session.settings.get("privacy_welcome_seen").is_none()` (line 640)
- Rendering at line 4544

But it's not being displayed on startup.

---

## Questions to Investigate

1. Is `privacy_welcome_seen` already set in settings?
2. Is the condition `privacy_welcome = ...` evaluating to false?
3. Is the dialog rendering code not being reached?
4. Is there a logic error in the dialog display flow?

---

## Why This Matters

The PrivacyWelcome dialog is the **only message** that should show after successful app startup to inform the user about logging level.

Without it, user has no way to know:
- What logging level is active
- Whether data is being collected
- What the debug messages mean

---

## Resolution

The startup condition now treats a missing or empty `privacy_welcome_seen` value
as first use. The fix is covered by tests for first launch, dismissal, the
`--first-use` flag, and settings-file removal.

## Historical Investigation Notes

1. Add logging to trace when `privacy_welcome_seen` is checked
2. Verify the condition evaluates correctly
3. Check if dialog is being rendered but not visible (UI bug)
4. Check if the setting is being set prematurely
