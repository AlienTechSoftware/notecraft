# Runtime troubleshooting workflow

## Startup failure

1. Run the same binary with `--version` to separate discovery from startup.
2. Locate the settings path and read the newest `notecraft.log`.
3. Check the first error, not only the final message. Startup failures often
   involve paths, graphics, window creation, clipboard, or settings parsing.
4. Check whether a panic backtrace or graphics/window-library warning appears.
5. Reproduce with a fresh, explicitly selected notebooks folder only if doing so
   will not overwrite user data.

## UI or graphics failure

Capture the platform, GPU/backend if available in the log, display scale, window
size, theme, and whether the failure occurs in a headless test or real window.
Use `nc-uiux-testing` for screenshot and UI automation evidence. A screenshot
cannot establish that a GPU or compositor is the cause.

## Notebook or save failure

Keep the notebook folder untouched. Record the operation, page/container path,
format, and whether the failure reproduces after reopening a copy. Do not repair,
rewrite, delete, or run broad cleanup commands as a diagnostic shortcut. Route
page-format and recovery questions through `nc-dom-ser` and `nc-prod`.

## Telemetry failure

Check the configured level, settings directory permissions, free space, and
whether the session directory and atomic manifest exist. Failure to write local
diagnostics should not stop the app. Do not turn on a remote service to work
around a local write failure.

## Reporting template

```text
Version/build:
Platform and display session:
Settings path:
Log path and time range (UTC):
Telemetry level:
Telemetry session ID:
Telemetry files inspected:
Reproduction steps:
Observed evidence:
Inference:
Unknown or unverified:
Sensitive data removed:
```
