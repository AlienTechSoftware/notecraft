@echo off
setlocal
cd /d "%~dp0"
cargo run --release -p notecraft -- --notebooks .\my-notebooks
