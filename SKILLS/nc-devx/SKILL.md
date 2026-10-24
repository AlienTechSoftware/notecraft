---
name: nc-devx
description: 'NoteCraft developer experience. One-time setup, development workflow, common tasks (lint, test, run app, git hooks), and platform-specific scripts. Fast feedback loop for local work.'
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [code-assistant, nc-arch, nc-code-rust]
---

# nc-devx - NoteCraft Developer Experience

Developer setup, workflow, and automation for local NoteCraft work.

---

## One-Time Setup

### Prerequisites

- **Rust:** [rustup.rs](https://rustup.rs) (latest stable)
- **Platform tools:** See platform-specific scripts below
- **Git hooks:** Enable once per clone

### Quick Setup (All Platforms)

Run the platform-specific setup script:

**Windows (PowerShell):**
```powershell
.\SKILLS\nc-devx\scripts\setup.ps1
```

**Linux:**
```bash
bash SKILLS/nc-devx/scripts/setup.sh
```

**macOS:**
```bash
bash SKILLS/nc-devx/scripts/setup.mac.sh
```

This script will:
1. Install Rust toolchain (if needed)
2. Install platform-specific tools (e.g., WiX on Windows)
3. Clone/fetch sub-repositories (icons, etc.)
4. Enable git hooks
5. Verify build and run a quick test

### Manual Setup

If you prefer manual steps:

```bash
# Enable git hooks (once per clone)
git config core.hooksPath .githooks

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Fetch sub-repositories (if needed)
# Example: git submodule update --init --recursive

# Verify setup
cargo build -p notecraft
cargo test --workspace
```

---

## Development Workflow

### Running the App

**Quick start (debug build, sample notebooks):**
```bash
cargo run -p notecraft
```

**Release build (optimized):**
```bash
cargo run --release -p notecraft
```

**With custom notebook folder:**
```bash
cargo run -p notecraft -- --notebooks /path/to/notebooks
```

**With bundled demo (after copy):**
```bash
cargo xtask demo-copy html
cargo run -p notecraft -- --notebooks ./my-notebooks --expand-all
```

### Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p notecraft-formats

# With output
cargo test --workspace -- --nocapture

# Run a specific test
cargo test test_icon_round_trip -- --exact
```

### Linting and Formatting

**Format check (pre-commit runs this):**
```bash
cargo fmt --all -- --check
```

**Auto-format:**
```bash
cargo fmt --all
```

**Clippy lint (catches style issues):**
```bash
cargo clippy --workspace
```

**Full pre-commit gate (before you commit):**
```bash
cargo xtask ci
```

### Common Commands

| Task | Command |
|------|---------|
| Build all | `cargo build --workspace` |
| Build release | `cargo build --release` |
| Run full CI gate | `cargo xtask ci` |
| Regenerate demo notebook | `cargo xtask demo` |
| Check demo parity | `cargo xtask demo-check` |
| Rebuild docs/images | `cargo xtask screenshots` |
| Check text style | `cargo xtask text` |
| Verify layers | `cargo xtask layers` |
| Check assets/ATTRIBUTION.md | `cargo xtask assets` |

---

## Personally Identifiable Information (PII) Scanner

NoteCraft checks for sensitive data before commits and pushes to prevent accidental leaks.

### What's Checked
- Owner name and email patterns
- Notebook file extensions (should not be in repo)
- SSN-like patterns (###-##-####)
- Custom patterns per project

### Manual Scan
```bash
# Scan current staged files
bash SKILLS/nc-devx/scripts/check-pii.sh

# Scan all files
bash SKILLS/nc-devx/scripts/check-pii.sh --all
```

### Fixing PII Issues

If the scanner blocks a commit or push:

1. **Identify the issue:**
   ```bash
   # The hook will show which files contain what patterns
   # Edit those files and remove the sensitive data
   ```

2. **Common fixes:**
   - Remove owner name/email from commits: Use `git commit --author` or `git config user.name`
   - Remove `.one` files: `git rm --cached notebook.one`
   - Remove personal data from comments: Edit the file and remove it
   - Update email references: Replace with generic placeholder

3. **Re-stage and commit:**
   ```bash
   git add <file>
   git commit  # Re-run pre-commit hook
   ```

4. **If it's in commit history:**
   ```bash
   # Amend the previous commit if not yet pushed
   git commit --amend
   
   # Or rebase to fix multiple commits
   git rebase -i HEAD~N  # Replace N with number of commits to go back
   ```

### Customize PII Patterns

Edit `SKILLS/nc-devx/scripts/check-pii.sh` to add project-specific patterns:

```bash
PII_PATTERNS=(
    "person@domain"            # Actual email to block (uncomment and update)
    "sensitive-word"           # Add your patterns here
)
```

### Cleaning PII from Git History

If PII was already committed (e.g., in commit author fields), the scanner will catch it in *files* but not in git metadata. To clean git history:

**Option 1: Rewrite history (careful - affects all contributors)**
```bash
# For local-only branches (not yet pushed):
git rebase -i HEAD~N  # Go back N commits, manually edit or squash

# For published branches, use git filter-repo:
# Install: pip install git-filter-repo
git filter-repo --replace-text patterns.txt  # See git-filter-repo docs
```

**Option 2: Accept and prevent future**
- Leave historical commits as-is (no public harm once committed)
- Prevent new commits with the scanner
- Update `.gitconfig` to use a safe author email

**Recommended:**
```bash
# Set per-project safe author
git config user.email "dev@company.com"
git config user.name "Developer"

# Verify
git config user.email
```

---

## Git Hooks

Hooks are in `.githooks/` and run automatically after `git config core.hooksPath .githooks`.

### Pre-commit Hook (runs every commit)
**File:** `.githooks/pre-commit`  
**Speed:** ~2 seconds  
**Checks:**
- `cargo fmt --check` (code style)
- `cargo xtask layers` (crate boundaries)
- `cargo xtask assets` (ATTRIBUTION.md)
- `cargo xtask docs` (link validity)
- `cargo xtask text` (plain ASCII, no fancy quotes)

**If it blocks:**
- Fix the issue and re-stage the files
- Or use `--no-verify` as a last resort (and fix the underlying issue)

### Pre-push Hook (runs before push to remote)
**File:** `.githooks/pre-push`  
**Speed:** ~few minutes  
**Checks:**
- Full `cargo xtask ci` (all gates: build, test, lint, layers, text, etc.)

**If it blocks:**
- Fix the issue locally and test: `cargo xtask ci`
- Then push

---

## Troubleshooting

### "cargo not found"
- Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Source your shell: `source ~/.cargo/env` (on Unix-like systems)

### "xtask not found"
- Run: `cargo build -p xtask`
- Xtask is a dev helper built on first use

### Pre-commit hook blocks on unrelated files
- Run `cargo xtask text` to see which files have issues
- Fix them, or use `git commit --no-verify` if they're pre-existing

### Slow build times
- Debug builds are optimized for speed; release builds take longer
- Use `cargo check` to verify without building binaries
- Use `cargo build -j 1` if you're out of memory

### Sub-repository not found
- Run the platform setup script again, or
- Manually update: `git submodule update --init --recursive`

---

## Platform-Specific Notes

### Windows
- **Setup script:** `SKILLS/nc-devx/scripts/setup.ps1`
- **Required tools:** Visual Studio Build Tools or MSVC, WiX (for installer builds)
- **Shell:** PowerShell recommended; Git Bash also works
- **Git hooks:** Use `CRLF` line endings (git config auto-handles)

### Linux
- **Setup script:** `SKILLS/nc-devx/scripts/setup.sh`
- **Required tools:** gcc/clang, pkg-config
- **Shell:** bash/zsh
- **Notes:** Some features (e.g., native dialogs) use X11 or Wayland

### macOS
- **Setup script:** `SKILLS/nc-devx/scripts/setup.mac.sh`
- **Required tools:** Xcode Command Line Tools, cargo-watch (optional)
- **Shell:** zsh (default in modern macOS) or bash
- **Notes:** Use `mac` in platform filenames (not `osx`)

---

## Quick Feedback Loop

For rapid iteration during development:

```bash
# Terminal 1: Watch and rebuild on changes
cargo watch -x 'check -p notecraft'

# Terminal 2: Run the app
cargo run -p notecraft

# Terminal 3: Run tests as you change them
cargo watch -x 'test -p notecraft-formats'
```

Or use your IDE's built-in build/test runner if available.

---

## Submitting Work

Before submitting a PR:

1. Run the full gate: `cargo xtask ci`
2. Fix any failures
3. Verify git hooks are enabled: `git config core.hooksPath` should show `.githooks`
4. Commit with clear messages
5. Push and open PR

See [AGENTS.md](../../AGENTS.md) for submission guidelines.

---

## References

- [AGENTS.md](../../AGENTS.md) - Rules, commands, and gates
- [Rust coding standards](nc-code-rust) - Code style, testing, error handling
- [Architecture](nc-arch) - System design and crate organization
- [Gaps and roadmap](../../docs/gaps.md) - Known limitations and future work
