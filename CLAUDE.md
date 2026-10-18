# CLAUDE.md

Read [`AGENTS.md`](AGENTS.md) first. It holds the rules for this repository.

Quick map:

- Code: `crates/` (layers L0 to L6), `apps/` (window and command line), `xtask/` (gates).
- Docs: [`docs/architecture.md`](docs/architecture.md), [`docs/testing.md`](docs/testing.md),
  [`docs/control-protocol.md`](docs/control-protocol.md), [`docs/gaps.md`](docs/gaps.md).
- Before finishing a change: run `cargo xtask ci` and look at any picture you changed.
