@echo off
REM ============================================================================
REM run.cmd - start NoteCraft from source for development (Windows).
REM
REM WHAT IT DOES
REM   1. Turns off command echo and keeps every variable set here local to this
REM      script (setlocal).
REM   2. Changes the current folder to the folder this script is in, so the
REM      cargo command always runs from the repository root, wherever you start it.
REM   3. Turns on debug logging for the egui UI crate (RUST_LOG).
REM   4. Runs the notecraft app with "cargo run" (a debug build), passing your
REM      extra arguments through to the app.
REM
REM REQUIREMENTS
REM   - Rust and cargo installed and on PATH (https://rustup.rs).
REM   - Run from anywhere; the script finds the repository itself.
REM
REM USAGE
REM   run.cmd                              start NoteCraft with the default setup
REM   run.cmd --first-use                  start again as if this were a first run
REM   run.cmd --new-version                show a newer version in Options (no download)
REM   run.cmd --new-version 0.0.14         same, naming the version to show
REM   run.cmd --set-version 0.0.8          run the real update check as version 0.0.8
REM   run.cmd --set-version 0.0.8 --update-from <folder>
REM                                        test an update from a local folder
REM   run.cmd "D:\Notes\Work"              open a notebook or folder given as a plain path
REM   run.cmd --open Notes/Work            open a page given with --open
REM   run.cmd --expand-all                 start with all tree items expanded
REM   run.cmd --control 9100               start the control channel on port 9100
REM   run.cmd --version                    print the app version and exit
REM
REM ARGUMENTS
REM   Anything you type after run.cmd is passed on to notecraft unchanged (the
REM   last argument on the cargo line of this script), after the script's own
REM   --notebooks option.
REM
REM   --notebooks FOLDER
REM     The notebooks folder. This script already passes ".\my-notebooks" (line 19).
REM     The app reads the FIRST --notebooks it sees, so a --notebooks you type here
REM     is ignored. To use another folder, edit line 19 instead.
REM     Without this flag the app uses the notebooks folder saved in its settings,
REM     or else its default folder (see notecraft_store settings: NOTECRAFT_NOTEBOOKS,
REM     then Documents\NoteCraft in your home folder).
REM
REM   --first-use
REM     Clears the saved "privacy welcome seen" flag so the first-use wizard
REM     shows again. Other settings are kept.
REM
REM   --new-version [VERSION]
REM     Test hook. Opens the update prompt as if a newer release existed. With
REM     VERSION it shows that version; without one, the app's built-in simulated
REM     version. Nothing is downloaded, the first-use wizard is skipped, and the
REM     first-use state is not changed.
REM
REM   --set-version VERSION
REM     Test hook. Runs the real GitHub update check as if the running app were
REM     VERSION, for this run only. Nothing is saved, so the next start is the
REM     real version again.
REM
REM   --update-from FOLDER
REM     Test hook. Checks for updates in a local folder instead of GitHub. The
REM     folder must hold SHA256SUMS.txt and a signed installer. Use it with
REM     --set-version to choose the version pretended to be running. No
REM     publishing takes place.
REM
REM   --prod
REM     Mentioned in an older note at the top of this file. No code in the
REM     repository reads it, so it has no effect. It is not used by this script.
REM
REM   Debug builds only (this script): with --new-version or --set-version, the
REM   request is handed to the installed copy of NoteCraft in the Program Files
REM   NoteCraft folder (notecraft.exe), when that copy exists, so the update
REM   runs against the installed program.
REM
REM LOGGING
REM   RUST_LOG is set to notecraft_ui_egui=debug, which turns on the debug
REM   messages from the UI crate (container rendering, image resize, and so on).
REM   The app's own log is written to notecraft.log in its settings folder. Its
REM   level is Debug when RUST_LOG contains debug or trace, or a "=debug" or
REM   "=trace" entry, and Info otherwise.
REM
REM BUILD PROFILE
REM   The script runs a debug build (no --release). Debug builds compile faster
REM   but run more slowly. For a release build, use the commented line above
REM   the active command (it adds --release) and comment out the active one.
REM
REM EXIT CODE
REM   The script exits with the exit code of cargo (and so of notecraft). A
REM   non-zero code means the build or the app failed.
REM ============================================================================


@REM  --prod -> simulate production
@REM  --set-version 0.0.8 -> simulate new version is 0.0.08
@REM --update-from <folder> -> simulate update from a local folder (SHA256SUMS.txt + a signed installer), no publishing
@REM  --first-use -> simulate first use


cd /d "%~dp0"
REM Enable debug logging for UI egui (container rendering, image resize, etc)
set RUST_LOG=notecraft_ui_egui=debug
REM Pass optional NoteCraft arguments through, for example: run.cmd --first-use
REM or run.cmd --new-version, to show a newer version in Options without downloading anything
REM Test an update from a local folder (SHA256SUMS.txt + a signed installer), no publishing:
REM run.cmd --set-version 0.0.8 --update-from <folder>
@REM  cargo run --release -p notecraft -- --notebooks .\my-notebooks %*
cargo run -p notecraft -- --notebooks .\my-notebooks %*
