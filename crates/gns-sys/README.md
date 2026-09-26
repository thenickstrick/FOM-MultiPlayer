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

## `ESteamNetworkingConfigValue`/`DataType`

Only the entries this workspace actually sets are bound; see the enum's
full definition in the pinned header for the rest.

## `SteamNetworkingIPAddr`

Under the same `#pragma pack(push,1)` region as `SteamNetworkingIdentity`.
Only needed as an opaque, correctly-sized blob (zeroed for "unknown", or
passed straight through by the flat helper functions), never interpreted
field-by-field on the Rust side — so the union's IPv4-mapped variant isn't
transcribed separately.

## `SteamNetConnectionInfo_t`

Transcribed field-for-field, including the explicit `m__pad1` padding the
real header inserts between `m_addrRemote` and `m_idPOPRemote`. `packed(1)`
throughout; every offset verified against the real compiled header in
`tests/layout.rs`.

## `SteamNetworkingConfigValue_t`

Not under any active pack pragma in the real header; plain `#[repr(C)]`
reproduces the real size/align/offsets exactly (verified in
`tests/layout.rs`) because every field here is already naturally aligned.

## Opaque interface handles

`ISteamNetworkingSockets` and `ISteamNetworkingConnectionSignaling` are
C++ classes with vtables; the flat API takes an opaque pointer to each as
a function argument and dispatches through the vtable on the C++ side. We
never need to know their layout — that's the entire point of the flat API
— so each is represented as an empty enum, the standard Rust idiom for an
FFI type only ever held as a pointer.

## Custom signaling: a plain-C bridge, not a hand-rolled vtable

GNS's custom signaling is normally a pair of C++ abstract classes
(`ISteamNetworkingConnectionSignaling`/`ISteamNetworkingSignalingRecvContext`)
that an app implements. Hand-rolling a Rust type with a real Itanium-ABI
vtable to satisfy those is exactly the kind of memory-unsafe FFI this
project's dependency policy exists to avoid.

The flat header already anticipates this: `CreateCustomSignaling`/
`ReceivedP2PCustomSignal2` are a plain-C bridge (`void *ctx` + function
pointers) to the same functionality, so that's what's bound here instead
— no vtable ABI risk. A C++ `const T&` reference parameter is also
ABI-identical to `const T*` (a non-null pointer) for every compiler this
project targets, so the callback typedefs below use pointer types in
place of the header's reference params.

## `build.rs`

Deliberately no `cc`/`cmake` helper crate (see `docs/dependency-policy.md`):
shells out to the system `cmake` directly via `std::process::Command` and
emits the link directives by hand.

Per-platform link libraries, beyond GNS itself, OpenSSL, and protobuf:

- **macOS**: `c++` (GNS is C++; the Rust toolchain's linker doesn't pull
  in libc++ automatically the way MSVC's does its runtime), plus the
  `CoreFoundation`/`Security` frameworks GNS's crypto/cert code touches.
- **Linux**: `stdc++` (same reason as macOS's `c++`) and `pthread`.
- **Windows**: `ws2_32`, `crypt32`, `winmm`, `iphlpapi` — GNS's own
  unconditional Windows link requirements (`src/CMakeLists.txt`'s
  `CMAKE_SYSTEM_NAME MATCHES Windows` branch): sockets, certificate store
  access, a high-resolution timer, and network interface enumeration for
  ICE candidate gathering. No explicit C++ runtime link needed here,
  unlike macOS/Linux — MSVC object files carry their own `/DEFAULTLIB`
  directives for the CRT/STL, which the linker honors automatically.

The static lib's output filename also differs by platform: MSVC's
convention is `NAME.lib`, not the Unix `libNAME.a` this CMake build
produces everywhere else — `build.rs` picks the right one to search for.

OpenSSL/protobuf discovery on Windows is unverified: it currently relies
on CMake's default `find_package` search, and a vcpkg install will likely
need `CMAKE_TOOLCHAIN_FILE` wired in some way this doesn't do yet.
