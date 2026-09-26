# FOM-MultiPlayer

Standalone Rust + GameNetworkingSockets (GNS) multiplayer mod for Fields of Mistria.

Replaces the legacy client↔relay-hub UDP design with GNS's P2P transport (NAT
traversal via built-in ICE, symmetric-connect mode, AES-GCM-256/Curve25519
encryption) while keeping a host-hub star topology and a small self-hosted
signaling service for rendezvous only — never game traffic.

Ships for **Linux and Windows** (Steam Play/Proton covers the game on
Linux). There is no macOS target; development on this repo happens to be
done on macOS, but nothing here is macOS-specific.

## Architecture

| Crate | What it is |
| --- | --- |
| `crates/json` | Hand-rolled JSON value type, parser, writer (no serde) |
| `crates/mp-proto` | Message envelope (`PlayerState`, `SnapshotRequest`, `Snapshot`) built on `json` |
| `crates/gns-sys` | Hand-written FFI bindings to GNS's flat C API |
| `crates/gns` | Safe Rust wrapper: connection lifecycle, custom P2P signaling, send/receive |
| `crates/mp-signal` | Room-code rendezvous relay — a small always-on TCP service |
| `crates/mp-relay` | The native companion process a game instance runs: shared-file loop ↔ GNS connection |
| `installer/mp-installer` | Standalone installer: finds the game + MOMI mods folder, writes the mod package, places the `mp-relay` binary |
| `third_party/GameNetworkingSockets` | Valve's GNS, vendored as a pinned git submodule |

See `docs/dependency-policy.md` for why this avoids third-party crates
(no serde, no tokio, no bindgen, no cc/cmake helper crates) and what's used
instead.

## Dependencies

Building from source needs:

- **Rust** (stable), via [rustup](https://rustup.rs)
- **CMake** ≥ 3.15 — builds the vendored GNS submodule
- **A C++17 compiler** — GNS is C++
- **OpenSSL** (dev headers/libs) — GNS's crypto backend
- **protobuf** (dev headers/libs) — GNS's rendezvous message encoding
- **git** — for the submodule

Platform notes:

- **Linux**: install via your package manager, e.g. `apt install cmake g++ libssl-dev libprotobuf-dev protobuf-compiler`.
- **Windows**: install CMake and a protobuf/OpenSSL toolchain (e.g. via [vcpkg](https://vcpkg.io)), plus MSVC (Visual Studio Build Tools) for the C++ compiler.
- **macOS** (dev-only, not a ship target): `brew install cmake openssl protobuf` — `gns-sys/build.rs` already looks these up via `brew --prefix`.

> **Known gap:** `gns-sys/build.rs` links GNS's own required Windows system
> libraries (`ws2_32`, `crypt32`, `winmm`, `iphlpapi`, per GNS's own
> CMakeLists.txt) and finds the MSVC-named static lib (`.lib`, not `lib*.a`).
> Not yet verified: OpenSSL/protobuf discovery on Windows currently relies on
> CMake's default `find_package` search (the `brew --prefix` lookup this
> build.rs uses for macOS obviously doesn't apply) — a vcpkg install will
> likely need `-DCMAKE_TOOLCHAIN_FILE` set some other way (e.g.
> `CMAKE_TOOLCHAIN_FILE` env var) for CMake to find them at all. Untested on
> a real Windows machine.

## Setup

```sh
git clone --recurse-submodules <this-repo>
# or, if already cloned without --recurse-submodules:
git submodule update --init --recursive

cargo build --workspace
```

The first build compiles the vendored GNS submodule via CMake, which takes
a while; subsequent builds are incremental.

## Running

**mp-signal** (the rendezvous server — run once, anywhere reachable by both peers):

```sh
cargo run -p mp-signal -- 7777   # port; defaults to 7777 if omitted
```

**mp-relay** (one per game instance — the host generates/shares a room code, the joining peer enters it):

```sh
cargo run -p mp-relay -- host <mp-signal-addr> <room-code> <shared-dir>
cargo run -p mp-relay -- join <mp-signal-addr> <room-code> <shared-dir>
```

**mp-installer** (finds the game + MOMI mods folder and installs the mod package):

```sh
cargo run -p mp-installer -- --gml-source <dir> [--relay-binary <path>]
```

## Testing

```sh
cargo test --workspace
```

`mp-relay`'s integration tests spawn the real, already-built `mp-signal`
binary as a subprocess, so run `cargo build -p mp-signal` first if you're
testing `mp-relay` in isolation (`cargo test --workspace` builds everything
it needs automatically).

## Status

Tracked in Linear (project: Fields of Mistria Multiplayer mod). As of this
writing: the JSON crate, mp-signal, the gns wrapper, mp-relay's shared-file
loop and GNS integration, the regression/soak test suite, and mp-installer
(discovery, packaging, legacy-mod migration) are implemented and tested.
The GML mod itself (the in-game side, written against MMAPI) has not been
started — it can only be meaningfully verified against a real game + MOMI
install.
