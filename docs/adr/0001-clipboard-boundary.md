# ADR 0001: The clipboard is a platform boundary

Status: accepted for the boundary. The Windows backend's dependency choice is open (see Open
questions).

## Context

NoteCraft copies and pastes many kinds of content: plain text, HTML, RTF, images, NoteCraft's own
page format, and URLs, as well as formats that other programs register under their own names. The
app must read and write every format a platform offers, and it must work with the clipboard the user
shares with other programs.

The app also needs tests that exercise copy and paste without depending on that shared clipboard,
so that they can run beside other tests.

## Options considered

1. **egui's clipboard only.** egui's copy writes text, and its paste event delivers text. This is
   the simplest option, and it needs no new code.
   - Rejected. egui offers text and nothing else. RTF, images, HTML, NoteCraft's own format and URLs
     cannot be read or written through it, so copy and paste would work for text only.
2. **arboard for every format.** The crate already in the build reads and writes text, HTML and
   images.
   - Kept for the formats it covers. It has no RTF, no registered custom formats and no URL formats,
     so it cannot be the whole answer.
3. **Win32 clipboard API directly, for every format.** The Windows clipboard can carry any format,
   including formats registered by name.
   - Accepted as the target for the Windows backend. The calls are unsafe and need careful wrapping.
4. **A Windows clipboard crate** (such as `clipboard-win` or the `windows` crate) in place of raw
   Win32 calls.
   - Open. A new dependency needs its licence recorded in `ATTRIBUTION.md`, and the owner has not
     yet chosen one.

## Decision

The clipboard is a boundary in the application, not a call scattered through it.

- The app holds one clipboard object, supplied at startup. The object is a trait that works in
  formats: it reads and writes named formats carried as bytes, so RTF, HTML, images, NoteCraft's own
  format, URLs and registered formats all go through the same boundary.
- Each platform has its own backend behind the trait. The Windows backend uses every format the
  Windows clipboard offers. Other platforms get their own backends later, and until then the app
  says that the feature is not built there.
- The app does not replace egui's copy and paste events for in-window editing. egui still handles
  keyboard events inside the window, and the boundary handles the system clipboard.
- If the clipboard cannot be opened at startup, the app still runs. The user sees an alert, and every
  clipboard action reports that the clipboard is unavailable.
- Tests use a double of the boundary, kept in the test folder. It is the only mock the rules allow
  (see `AGENTS.md`, the clipboard exemption). Product code has no test hook for it.

## Consequences

- Text-only copy and paste would not satisfy the app's requirements, so egui's clipboard is not used
  for the system clipboard.
- The first backend (what we have now, built on arboard) covers text, HTML and images. RTF, custom
  and URL formats are not yet read or written. Each of those is a gap until the Windows backend is
  done, and each must be listed in `docs/feature-parity.md` and `docs/gaps.md` until then.
- The double must model formats, not just text, so that a test checks what the app writes in each
  format.

## Open questions

- **The Windows backend's dependency:** raw Win32 calls, the `windows` crate, or `clipboard-win`.
  The owner decides, and the licence goes in `ATTRIBUTION.md` before the code lands.
- **The format list:** which registered formats NoteCraft owns (its own page format and the URL
  formats) and how they are named.
- **Other platforms:** macOS and Linux backends, when they are built.
