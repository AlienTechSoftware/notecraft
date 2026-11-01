@echo off
setlocal
cd /d "%~dp0"
REM Enable debug logging for UI egui (container rendering, image resize, etc)
set RUST_LOG=notecraft_ui_egui=debug
REM Pass optional NoteCraft arguments through, for example: run.cmd --first-use
cargo run --release -p notecraft -- --notebooks .\my-notebooks %*
