# NoteCraft patch

This is `mermaid-rs-renderer` 0.3.1, distributed under the MIT license in
`LICENSE`. It is kept here until an upstream release contains the WASM-safe
timer change.

`src/wasm_time.rs` substitutes a zero-duration timer on `wasm32-unknown-unknown`
for the internal layout metrics. Those metrics do not affect the SVG. Native
builds continue to use `std::time::Instant`.

When updating this copy, retain the timer shim at the upstream layout timing
call sites or remove this local Cargo patch once the released crate fixes them.
