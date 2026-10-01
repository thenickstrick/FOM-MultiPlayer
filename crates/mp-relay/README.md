# mp-relay

The native companion process a Fields of Mistria instance runs alongside
the game: polls the shared-file handoff directory and translates it
to/from a GNS P2P connection, rendezvousing through mp-signal.

## Usage

```sh
mp-relay <host|join> <mp-signal-addr> <room-code> <shared-dir>
```

## Shared-file contract

Subject to change once the GML mod actually consumes it. `out.json` is
local player state the mod writes and we read; `remote.json` is the
peer's player state, which we write; `world_snapshot_out.json` is a
snapshot the mod wants sent, which we read once per change;
`world_snapshot.json` is an incoming snapshot, which we write.

## `file_store`: shared-file read/write primitives

Mirrors the pattern proven in the legacy .NET relay's `RelayFileStore`.

- **`read_shared`**: retries on transient errors, since another process
  may briefly hold the file (especially on Windows, the actual target
  platform). A missing file is `Ok(None)`, not an error — the relay/mod
  polling loop treats "not written yet" as routine, not exceptional.
- **`write_atomic`**: writes to a `.tmp` suffix, then renames over the
  real path, so a concurrent `read_shared` only ever sees the old complete
  contents or the new complete contents, never a partial write. The
  rename step itself is retried on transient errors, matching the legacy
  relay's handling of brief antivirus/indexer locks on Windows; if
  renames keep failing, falls back to a direct (non-atomic) write so a
  stuck lock on the tmp file doesn't wedge the relay indefinitely.
- **`cleanup_temp_files`**: best-effort removal of stray `.tmp` files left
  behind by a crash mid-write. Errors removing an individual file are
  ignored, since another process may legitimately be writing it right now.

## `signal_link`: mp-signal's wire protocol, client side

Length-prefixed frames, host/join registration, then opaque relay — see
`crates/mp-signal`. Duplicated here rather than shared via a library
crate: it's the small, stable client half of a protocol whose server half
lives in a binary-only crate.

## `relay`: wiring it all together

`run`'s `initiate` picks which side calls `connect_p2p_custom_signaling`
(true) vs. only reacts to incoming signals (false) — see the `gns`
crate's README on why one side must initiate. The join side initiates;
the host side reacts, matching mp-signal's own host/join distinction.

### Tests

`SignalServer` spawns the real, already-built `mp-signal` binary on an
ephemeral port (requires `cargo build -p mp-signal` first if testing
`mp-relay` in isolation; `cargo test --workspace` builds it automatically).

The integration test rendezvous-es both sides through mp-signal *before*
driving either: a real host only shares its code once mp-signal has
confirmed registration, so a join attempt never races the host's
registration in practice, and the test shouldn't either.
