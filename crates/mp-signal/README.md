# mp-signal

Minimal always-on TCP service implementing GNS's pluggable custom-signaling
contract: room-code registration plus opaque rendezvous-blob relay between
two peers. Never touches game traffic.

## Protocol

Length-prefixed frames (`u32` big-endian length + payload) over plain
blocking `std::net`, thread-per-connection — no async runtime, no
websocket crate (see the workspace's `docs/dependency-policy.md`).

A host registers a room code; its socket is parked in a waiting-room map.
A join with the same code claims it, and the two sockets are spliced into
a bidirectional opaque frame relay. Unclaimed registrations are swept by a
background reaper after a timeout.

## Plaintext by design

This channel only ever carries ICE candidate/rendezvous metadata, and the
GNS connection it bootstraps is separately, unconditionally encrypted
regardless. See `docs/dependency-policy.md` for the fuller reasoning.
