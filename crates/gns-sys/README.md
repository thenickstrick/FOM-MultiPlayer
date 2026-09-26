# gns-sys

Hand-written FFI bindings to GameNetworkingSockets' (GNS) flat C API
(`include/steam/steamnetworkingsockets_flat.h`).

## Why hand-written, not bindgen

No `bindgen` (see the workspace's `docs/dependency-policy.md`): the flat
API is small, deliberately stable, and plain `extern "C"` (confirmed by
reading the header itself), so hand-writing declarations against the
pinned submodule commit is a bounded, auditable amount of code. Only the
surface actually used elsewhere in this workspace is bound; extend as
needed by reading the real header, transcribing the layout exactly, and
adding a layout-test case in `tests/layout.rs`.

## Struct layout verification

Every struct defined here has a matching layout test in `tests/layout.rs`
that checks `size_of`/`align_of`/`offset_of!` against values captured from
the pinned header at the time these bindings were written. A future
re-pin that changes the ABI shape must fail that test, not corrupt memory
silently.

To re-capture these numbers after a re-pin, compile and run a probe
against the real header:

```sh
c++ -std=c++17 -I third_party/GameNetworkingSockets/include/steam -o /tmp/probe probe.cpp
```

using a probe that prints `sizeof`/`alignof`/`offsetof` for the structs in
`tests/layout.rs` (see this file's git history for the exact probe used).

## `SteamNetworkingIdentity` layout

`{ int m_eType; int m_cbSize; union { ...; uint32 m_reserved[32]; }; }`
under `#pragma pack(push, 1)` (the real header pushes pack(1) at line 198
and doesn't pop it until line 348, after this struct). The loopback smoke
test never constructs or reads a real identity (`CreateSocketPair` takes
`nullptr` for both peer identities and assumes generic "localhost"), so
this only needs to be layout-correct: 4 (`m_eType`) + 4 (`m_cbSize`) + 128
(the union, sized by its largest member, `uint32 m_reserved[32]`) = 136
bytes.

`packed(1)` matches the real pragma exactly. It happens to make no
difference to the byte layout today, since every field is already
naturally 4-aligned, but a future field added to this struct might not be
— matching the source of truth avoids depending on that coincidence.

## `SteamNetworkingMessage_t`

Transcribed field-for-field, in order, including the two function-pointer
fields. `Release()` is a C++ inline method in the real header
(`m_pfnRelease(this)`); `SteamNetworkingMessage_t::release()` replicates
that call manually rather than declaring it as callable from Rust.

## Opaque interface handles

`ISteamNetworkingSockets` is a C++ class with a vtable; the flat API takes
an opaque pointer to it as every function's first argument and dispatches
through the vtable on the C++ side. We never need to know its layout —
that's the entire point of the flat API — so it's represented as an empty
enum, the standard Rust idiom for an FFI type only ever held as a pointer.

## `build.rs`

Deliberately no `cc`/`cmake` helper crate (see `docs/dependency-policy.md`):
shells out to the system `cmake` directly via `std::process::Command` and
emits the link directives by hand.

Per-platform link libraries, beyond GNS itself, OpenSSL, and protobuf:

- **macOS**: `c++` (GNS is C++; the Rust toolchain's linker doesn't pull
  in libc++ automatically the way MSVC's does its runtime), plus the
  `CoreFoundation`/`Security` frameworks GNS's crypto/cert code touches.
- **Linux**: `stdc++` (same reason as macOS's `c++`) and `pthread`.
