# Demo data does not persist after reload

**Status:** Implemented locally; awaiting deployment
**Area:** `notecraft.cloud/demo`

## Problem

After accepting or denying the first-run privacy dialog, reloading the standalone web demo shows the dialog again. The first page render can also leave the default text column too narrow when the view starts at a small size.

## Expected

The consent choice and selected collection level remain available after reload in the same browser. A fresh browser profile still gets the first-use dialog. The unarranged page layout responds to the view width.

## Observed

The consent dialog returns after accepting and pressing F5. The notebook appears to restart from its initial state.

## Source finding

The web persistence code in `.github/web-build/web.diff` enables notebook persistence only when the host page provides the OpenPlayground `playgroundFiles` API (`files`, `write`, and `remove`). The standalone demo does not provide that API. The fix stores standalone preferences in browser local storage; invitation sessions remain transient.

The combined production-profile WASM build was verified locally. The public demo remains unchanged until deployment completes.

## Acceptance criteria

- The privacy choice and selected collection level are retained across reloads in the standalone demo.
- Playground-hosted notebooks continue using its Files API; invitation sessions remain transient.
- An unarranged page expands after a narrow initial viewport.
- A fresh browser profile still gets the first-run dialog.
- Persistence behavior remains defined when browser storage is unavailable or cleared.

## Follow-up

After deployment, verify the public site and close this issue. Browser reload coverage is recorded in the v0.0.9 web UI automation work.
