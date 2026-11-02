---
name: nc-runtime-diagnostics
description: Diagnose NoteCraft runtime failures using its local logs, telemetry sessions, settings, manifests, event records, and privacy-preserving evidence workflow.
license: MIT
metadata:
  version: "0.0.20261101"
  related_skills: [nc-prod, nc-code-rust, nc-devx, nc-arch, nc-reelase-signoff]
---

# nc-runtime-diagnostics

## Job to be done

Diagnose NoteCraft runtime failures from local logs, sessions, settings, manifests, and privacy-preserving evidence.

Use this skill when an agent must understand a NoteCraft failure from runtime
evidence, find logs or telemetry, correlate an issue with a session, explain
what data is local, or add diagnostic behavior. It is the canonical operational
guide for NoteCraft's diagnostics system.

This skill describes the current implementation. If the code and this document
disagree, trust the code, record the discrepancy, and update this skill with the
same change that changes the behavior.

## Ownership and routing

This skill owns:

- application log and telemetry locations;
- runtime profiles and local collection levels;
- the difference between the engine notebook `Session` and the telemetry
  recording `Session`;
- session IDs, manifests, event records, rotation, redaction, export, and clear;
- diagnostic collection and troubleshooting workflows;
- safe evidence handling and reporting.

It does not own product policy. Route as follows:

- `nc-prod`: privacy, consent, local-only collection, user-facing promises, and
  whether a new event is product-appropriate;
- `nc-code-rust`: logging implementation, error handling, tests, and Rust
  quality;
- `nc-devx`: developer machine setup, WSLg, build profiles, hooks, and CI;
- `nc-arch`: ownership and lifecycle of the engine `Session` and data flow;
- `nc-reelase-signoff`: release verification and evidence requirements.

## Core invariants

1. Logs and telemetry are local. NoteCraft does not upload them automatically.
2. Notebook content is not intentionally logged, but panic text or error text
   can still reveal content being processed. Inspect before sharing.
3. Never ask a user to attach an entire settings or telemetry directory without
   first identifying the relevant files and checking for paths or personal data.
4. Preserve the original evidence. Work on a copy when redacting or extracting.
5. Correlate a report with the app version, build profile, platform, session ID,
   exact reproduction steps, and the relevant log/event time range.
6. Do not infer a telemetry event from a log line or an engine session from a
   telemetry session. They are different records with different purposes.

## Fast triage

When a runtime issue is reported:

1. Identify the build and platform. Ask whether it is debug, release, or an
   installed production build.
2. Find the active settings file and its sibling `notecraft.log` and `telemetry`
   directory. Do not assume the Windows path when running Linux or WSL.
3. Record the visible Session ID from About or Options, or read
   `telemetry_session_id` from settings if the UI cannot open.
4. Inspect `notecraft.log` from startup through the failure, including the
   rotated `.old` file if the event predates the current log.
5. Inspect only the matching telemetry session's `manifest.json` and
   `events.jsonl` when the collection level permits the event.
6. Compare timestamps in UTC. Reproduce once with the smallest safe diagnostic
   level that can observe the fault; do not default to Debug for a user's data.
7. Report what was observed, what was not collected because of the level, and
   what remains unverified.

Read the focused reference before taking action:

- [Paths and files](references/paths-and-files.md)
- [Telemetry and session schema](references/telemetry-and-sessions.md)
- [Privacy and evidence handling](references/privacy-and-evidence.md)
- [Troubleshooting workflow](references/troubleshooting.md)

## Diagnostic change rules

- Add stable, low-cardinality event names and named fields; do not concatenate
  arbitrary user text into event names.
- Redact paths and control characters before recording values. Do not add page
  bodies, notebook titles, clipboard contents, secrets, or full exception text
  to usage events without explicit product approval.
- Keep normal user-facing errors useful without exposing raw paths or internal
  implementation details. Put technical context in the local log.
- Add a regression test for new level filtering, redaction, session correlation,
  rotation, export, or clear behavior.
- Keep diagnostics failure-tolerant: failure to write a log or telemetry event
  must not prevent NoteCraft from opening or saving a notebook.

## Completion checklist

- [ ] The evidence source and build profile are identified.
- [ ] The settings, log, and telemetry paths are verified rather than guessed.
- [ ] The session ID and UTC time range are recorded when available.
- [ ] Shared artifacts were inspected for personal paths and notebook content.
- [ ] Privacy and product implications were routed to `nc-prod`.
- [ ] Code changes have focused tests and the relevant repository gate.
- [ ] The report distinguishes observed facts, inferences, and unknowns.
