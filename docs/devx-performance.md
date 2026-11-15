# Developer workflow timing

NoteCraft measures the time spent in its build and test gates so a slower developer workflow is
visible. This is a timing benchmark of the real validation commands, not a microbenchmark of app
code.

## Commands and measurements

`cargo xtask ci` records the duration of each L2 phase, including the parallel non-UI tests and
the serial UI tests. `cargo xtask test` records those two test phases. The Linux, Windows, and
macOS CI build/test job wraps `cargo build --workspace` and `cargo test --workspace` and adds
their durations to the GitHub Actions summary.

For another command, use:

```text
cargo xtask devx-time PHASE -- COMMAND [ARGS...]
```

For example, `cargo xtask devx-time build -- cargo build --workspace --locked` measures a
workspace build.

## Local history and privacy

Local records are written to the ignored file `target/devx/timings.jsonl`. They include the task,
phase names, elapsed milliseconds, success state, operating system, Rust compiler version, and a
timestamp. They do not include a machine name, user name, branch name, notebook path, or note
content. The app's runtime telemetry is separate. Local timing records are never uploaded.

CI keeps a separate timing history in the GitHub Actions cache, keyed by platform and the Rust
toolchain file. Successful `main` build/test jobs add samples. Pull request jobs restore that
history, report timings and warnings, and do not save their samples. This keeps local computer
measurements out of GitHub while allowing the hosted runners to compare like with like.

## Warning policy

Comparisons use only successful samples for the same task, platform, and Rust compiler version.
At least three prior samples are required. The reference is the slowest of the latest five
successful samples, which reduces warnings from variable runner load. A phase warns only when it
takes more than 25 percent and at least 30 seconds longer than that reference. Warnings appear in
the console, and in CI as an annotation and summary entry. They never fail a build or test gate.

The first runs on a new machine or runner have no baseline. CI timing history also starts empty
when this feature is first enabled and warms as successful `main` runs populate the cache.
