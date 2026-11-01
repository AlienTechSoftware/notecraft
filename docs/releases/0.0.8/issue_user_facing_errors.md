# User-Facing Error Messages - Sanitize Technical Details

- Date: 2026-10-31
- Status: Bug report
- Branch: `feat/v0.0.8`
- Severity: High (user confusion, bad UX)

---

## Problem

Users see raw OS errors with technical paths and error codes during normal app operation. These are **internal failures** that should be handled gracefully with user-friendly messages.

### Example Error Shown to User

```
The Welcome notebook could not be deployed to \\?\UNC\VBoxSvr\C_DRIVE\dev\p-NoteCraft\.repos\o-NoteCraft_2\my-notebooks: 
More data is available. (os error 234)
```

**Problems:**
- ❌ Shows raw VirtualBox UNC path (`\\?\UNC\VBoxSvr\...`)
- ❌ Shows cryptic OS error codes (`os error 234`)
- ❌ Uses technical jargon ("deploy")
- ❌ User has no idea what to do
- ❌ Looks like an installation/setup failure

---

## Expected User Message

```
Couldn't create the Welcome notebook in your notebooks folder.

This might be a temporary issue. NoteCraft will continue without it, but you won't have the tutorial.

[Retry]  [Continue Anyway]
```

**Better because:**
- ✅ Clear, simple language
- ✅ No technical paths or error codes
- ✅ Explains the impact (won't have tutorial)
- ✅ Gives the user options
- ✅ Suggests it might be temporary

---

## Why Is This Happening?

The app tries to deploy a "Welcome notebook" at startup (some kind of tutorial or demo).

**Design questions:**
- Should this happen at all?
- Should it fail silently if it can't deploy?
- Should it only happen on first launch?
- Should it be optional/skippable by user?

Current behavior: Tries to deploy, fails, shows ugly error.

Better behavior: Either:
1. Don't deploy it (remove the feature)
2. Deploy silently, fail silently (don't show error)
3. Show a **user-friendly** message if needed

---

## Root Cause

Location: `apps/notecraft/src/main.rs` line 157

```rust
Err(e) => {
    let message = format!("The Welcome notebook could not be deployed to {}: {e}", 
                          session.root().display());  // ← raw error + full path
    eprintln!("{message}");
    startup_alerts.push(("welcome-notebook-deploy", message));
}
```

When notebook deployment fails (file I/O, permissions, etc.), the raw error is passed to the user.

---

## Solution Approach

### Pattern to Apply Everywhere

**Before (Bad):**
```rust
Err(e) => {
    let msg = format!("Failed to do X: {e}");  // Raw error exposed
    show_to_user(&msg);
}
```

**After (Good):**
```rust
Err(e) => {
    log::error!("Failed to do X: {e}");  // Technical details logged
    let user_msg = match e.kind() {
        io::ErrorKind::PermissionDenied => "You don't have permission to do X.",
        io::ErrorKind::NotFound => "The folder couldn't be found.",
        _ => "Couldn't do X. This might be temporary; try again.",
    };
    show_to_user(user_msg);
}
```

### For Notebook Deployment Specifically

File: `apps/notecraft/src/main.rs` line 157

Change to:
```rust
Err(e) => {
    log::error!("Welcome notebook deployment failed: {e}");
    let user_msg = "Couldn't create the Welcome notebook in your notebooks folder. \
                    You can create it later from the File menu. NoteCraft will continue.";
    startup_alerts.push(("welcome-notebook-deploy", user_msg.to_string()));
}
```

---

## Acceptance Criteria

- [ ] No raw OS error codes shown to users
- [ ] No VirtualBox UNC paths shown to users
- [ ] No technical jargon ("deploy", "io error") in user messages
- [ ] All startup errors have user-friendly text
- [ ] Technical details logged (for developers)
- [ ] User messages suggest next steps or workarounds

---

## Files to Audit

- `apps/notecraft/src/main.rs` - startup errors (line 157+)
- `crates/ui-egui/src/app.rs` - dialog messages
- `crates/ui-egui/src/backstage.rs` - file operation errors
- Search for: `format!.*{e}` and `{error}` patterns that show to users

---

## Related

This affects user trust. When users see technical errors, they:
- Lose confidence in the app
- Think it's a "power user" tool, not an everyday app
- Don't know if something is actually broken or just a hiccup
