# NoteCraft diagnostic paths and files

## Resolve the settings file

The application uses this precedence, implemented by
`crates/store/src/settings.rs`:

1. `NOTECRAFT_SETTINGS`, if set.
2. `local/settings.toml` when it exists. Development checkouts also prefer
   this path when the current directory contains `local/` or `Cargo.toml`.
3. Windows: `%APPDATA%\NoteCraft\settings.toml`.
4. Linux and other XDG systems: `$XDG_CONFIG_HOME/notecraft/settings.toml`.
5. Otherwise: `$HOME/.config/notecraft/settings.toml`.
6. If no home/config environment is available: `notecraft-settings.toml` in the
   current directory.

The settings file is not stored in a notebook. It is a key/value text file and
may contain `telemetry_level`, `telemetry_session_id`, privacy state, the last
notebooks root, and other preferences. Treat it as personal configuration.

## Files beside settings

The desktop app initializes the log beside the settings file:

```text
<settings directory>/notecraft.log
<settings directory>/notecraft.log.old
<settings directory>/telemetry/<session-id>/manifest.json
<settings directory>/telemetry/<session-id>/events.jsonl
```

The telemetry directory is created when a telemetry session starts. A `None`
level normally means no session is started, so an absent telemetry directory is
not automatically an error.

The log rotates the existing `notecraft.log` to `notecraft.log.old` when it is
over 1,000,000 bytes at app initialization. Rotation is a single old file, not
an unbounded archive.

## Useful commands

Use environment-aware commands rather than hard-coding Windows paths:

```text
notecraft --version
notecraft-cli --version
```

For a development run, inspect the resolved settings path in the startup log.
Do not confuse the WSL path `/mnt/c/...` with the Windows process's `%APPDATA%`
path. A Windows build and a Linux/WSL build can have different settings and
telemetry roots.
