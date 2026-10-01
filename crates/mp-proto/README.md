# mp-proto

Shared-file/GNS message protocol: `PlayerState` and snapshot framing — the
shapes `mp-relay` reads/writes and the GML mod expects.

Discriminator-keyed generic JSON, mirroring the legacy C# relay's
`RelayMessageParser` (dispatch on which key is present — `player_id` vs.
`mp_msg` — rather than a typed envelope with a tag field). Unlike the
legacy relay, there's no chunked `snap_begin`/`snap_end` framing: GNS
fragments/reassembles large reliable messages itself, so a snapshot is
just one message.

## `RelayMessage::PlayerState`

Per-frame local player state. High-frequency, latest-value-wins, so sent
unreliable. `payload` excludes `player_id` — it's re-inserted by
`to_json`, so a `payload` that also carried it would duplicate it.

## `RelayMessage::SnapshotRequest`

Asks the peer to send a full `Snapshot`. Sent reliable: unlike player
state, a dropped request just means "nobody asked again", which the
request loop should treat as a retry condition rather than something
recoverable at the message layer.
