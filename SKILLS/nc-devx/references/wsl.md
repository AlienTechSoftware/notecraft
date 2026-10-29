# WSL2 development

NoteCraft builds on WSL2 with the pinned Rust toolchain and the Linux desktop
development packages. WSLg provides the display sockets needed by the app, but
an agent-launched process may not have the same compositor access as an
interactive WSL terminal.

## Prepare Ubuntu

From the repository root, run the normal Linux setup check:

```bash
CHECK_ONLY=1 bash SKILLS/nc-devx/scripts/setup.sh
```

If packages are missing, run the setup script without `CHECK_ONLY` from an
interactive WSL terminal so `sudo` can prompt. In an automation session where
sudo cannot prompt, the script uses the WSL root launcher when it is available:

```bash
wsl.exe -u root -- apt-get update
wsl.exe -u root -- apt-get install -y build-essential pkg-config \
  libxkbcommon-dev libwayland-dev libx11-dev \
  libfontconfig1-dev libfreetype6-dev
```

The root launcher changes the Ubuntu package database and installs packages for
this WSL distribution. Use it only for the package installation step. Do not
run the application as root.

## Build and test

```bash
cargo build -p xtask
cargo build -p notecraft
cargo test --workspace
```

For the full repository gate:

```bash
cargo xtask ci
```

## Launching the UI

Check the WSLg environment before launching:

```bash
printf 'DISPLAY=%s\n' "$DISPLAY"
printf 'WAYLAND_DISPLAY=%s\n' "$WAYLAND_DISPLAY"
printf 'XDG_RUNTIME_DIR=%s\n' "$XDG_RUNTIME_DIR"
ls -l /tmp/.X11-unix
ls -l /mnt/wslg 2>/dev/null | head
```

Launch normally first:

```bash
cargo run -p notecraft
```

If Wayland reports `Connection(NoCompositor)`, retry through X11:

```bash
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 cargo run -p notecraft
```

A successful compile or a process reaching `Running target/debug/notecraft`
does not prove that a window was visible. Confirm the window from an
interactive WSL terminal with WSLg, and report build and UI results separately.
