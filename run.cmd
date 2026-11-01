@echo off
setlocal
cd /d "%~dp0"
REM Enable debug logging for UI egui (container rendering, image resize, etc)
set RUST_LOG=notecraft_ui_egui=debug
REM --first-use shows the privacy welcome again and keeps every other setting
cargo run --release -p notecraft -- --notebooks .\my-notebooks
@REM  cargo run --release -p notecraft -- --first-use --notebooks .\my-notebooks
