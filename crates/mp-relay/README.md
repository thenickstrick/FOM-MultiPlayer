# mp-relay

The native companion process a Fields of Mistria instance runs alongside
the game: polls the shared-file handoff directory and translates it
to/from a GNS P2P connection.

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
