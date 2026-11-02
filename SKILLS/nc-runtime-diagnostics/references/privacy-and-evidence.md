# Privacy and diagnostic evidence

## Product promise

NoteCraft's current product rule is local-only diagnostics. Logs and telemetry
remain on the device and are not uploaded automatically. This rule belongs to
`nc-prod`; this reference explains how agents must handle the resulting files.

## Before sharing

Inspect and redact:

- usernames, home directories, drive letters, UNC paths, and notebook paths;
- page titles, file names, clipboard text, import sources, and attachment names;
- email addresses, tokens, URLs containing credentials, and environment values;
- panic text that may include the content being processed.

The runtime redactor protects telemetry field values, not every possible value
in the application log. Never assume `notecraft.log` is safe merely because the
telemetry event writer redacts paths.

## Safe evidence bundle

Prefer the smallest bundle that answers the question:

1. application version and platform/build profile;
2. relevant `notecraft.log` lines, with sensitive values redacted;
3. matching `manifest.json`;
4. only the relevant `events.jsonl` records;
5. reproduction steps and UTC timestamps;
6. a screenshot only when it adds information not present in the text.

If the user exports a telemetry ZIP from the app, treat it as untrusted personal
data. Inspect its contents before attaching it to an issue or sending it to an
agent. Do not upload or post it automatically.

## Collection-level honesty

An absent event does not prove that the action did not happen. It may have been
filtered by `telemetry_level`, failed to write, or not yet been instrumented.
State the configured level and the observed files in the report.

## Changes to collection

Changing the default level, adding a new event category, or changing consent
copy is a product/privacy decision. Consult `nc-prod` before implementation.
Do not add a network exporter, remote crash reporter, or background upload as a
diagnostic convenience.
