# Dependency policy

This project avoids third-party dependencies wherever avoidable. Where something genuinely isn't native to Rust (i.e. not in `std`) and we need it, we write and maintain it ourselves rather than pull in an external crate or library whose provenance we don't fully control.

Concretely, this project does **not** use: `serde`/`serde_json`, `tokio` or any async runtime, a websocket crate, `bindgen`, or `cc`/`cmake` build-helper crates.

| Need | What we'd normally reach for | What we do instead |
| ---- | ---- | ---- |
| Talk to GNS's C API from Rust | `bindgen` | Hand-written `extern "C"` declarations in `crates/gns-sys/src/lib.rs` against GNS's small, stable flat API. Every struct has a matching layout test (`crates/gns-sys/tests/layout.rs`) checked against a probe compiled directly against the real pinned header (`crates/gns-sys/tests/layout_probe.cpp`). |
| Compile/link GNS from `build.rs` | `cc`/`cmake` crates | `crates/gns-sys/build.rs` shells out to the system `cmake` via `std::process::Command` directly and emits `cargo:rustc-link-*` manually. |
| Parse/write the shared-file JSON protocol | `serde`/`serde_json` | A hand-rolled generic JSON value type in `crates/json` (mirrors how the legacy relay treated `out.json` as a generic mutable tree, not a typed DTO). |
| Signaling channel transport | `tokio` + a websocket crate | Plain blocking `std::net::{TcpListener, TcpStream}` in `mp-signal`, one thread per connection. |
| GNS itself (C++) | A crates.io `*-sys` wrapper of unknown provenance, or depending on this workspace's sibling `GameNetworkingSockets/` checkout | Our own pinned git submodule under `third_party/GameNetworkingSockets`, vendored into this repo directly. |

## Two open tensions, not silently resolved

1. **Hand-written FFI bindings are a real ongoing maintenance cost.** Mitigated by pinning the GNS submodule to an exact commit and the layout-test guard above, which fails loudly if a re-pin changes the ABI shape rather than corrupting memory silently.
2. **The signaling channel (`mp-signal`) has no transport encryption** under this policy, since hand-rolling TLS is a different risk category than hand-rolling a JSON parser. Current position: plaintext is acceptable, because the channel only ever carries ICE candidate/rendezvous metadata, never gameplay data, and the GNS connection it bootstraps is separately, unconditionally encrypted (Curve25519 + AES-GCM-256) regardless. Revisit if that assessment changes.

See the full project plan for more context on why these choices were made.
