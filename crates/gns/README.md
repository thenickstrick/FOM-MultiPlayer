# gns

Safe Rust wrapper over `gns-sys`: connection lifecycle, custom P2P
signaling, and message send/receive.

## Design

Deliberately signaling-transport-agnostic: `connect_p2p_custom_signaling`
and `receive_signal` take plain closures for sending/receiving opaque
rendezvous blobs, so this crate has no knowledge of mp-signal's wire
protocol (see the `mp-relay` crate for that integration). Connection
state is polled via `GetConnectionInfo` rather than a registered
status-changed callback, matching the poll-loop style used throughout
this workspace (`mp-signal`/`mp-relay`) and avoiding a second FFI
callback path.

## `SignalSender`

A boxed opaque-blob sender: called with a rendezvous message that must be
delivered to the peer out-of-band (e.g. relayed through mp-signal). GNS
may invoke this from its own internal thread, at any time for the life of
the connection, per the underlying API's documented contract.
Implementations must not block or call back into `Gns`/`GnsConnection`.

## `Gns::init`

Initializes the standalone (non-Steam) GNS library, process-wide. Safe to
call more than once (e.g. from independent tests in the same binary) —
only the first call does the real work.

## `Gns::run_callbacks`

Pumps GNS's internal callback queue. Call this regularly from the same
poll loop that drives everything else; GNS's underlying networking
thread runs independently, but connection bookkeeping is only serviced
when this is called.

## `connect_p2p_custom_signaling`

`on_signal` is called (possibly from GNS's own thread) with each
rendezvous blob that must be delivered to the peer; feed the peer's
replies back in via `receive_signal`.

`symmetric` sets `k_ESteamNetworkingConfig_SymmetricConnect`, but that
alone is not sufficient for a symmetric connection to reach `Connected`:
GNS also requires a matching `CreateListenSocketP2P` on the same local
virtual port, tagged the same way. Virtual-port allocation is
topology-specific (who's a hub vs. a spoke), so that call is left to the
integration layer (`mp-relay`) rather than made here.

## `receive_signal`

If the blob represents a request for a new inbound connection,
`on_connect_request` is called synchronously, before this returns, to
obtain a signal sender for it; return `None` to silently ignore the
request (see `ISteamNetworkingSignalingRecvContext::OnConnectRequest`'s
docs on why ignoring, rather than actively rejecting, is the safer
default).

## FFI trampolines: `signal_sender_into_ctx`

Double-box: `Box<dyn Trait>` is a fat pointer, which doesn't fit in a
`*mut c_void`. Boxing it again gives a thin pointer to that fat pointer,
which does. Reclaimed in `release_trampoline`.

## Tests: loopback signaling

`wire_loopback_signaling`/`connect_pair` simulate what mp-signal does
across two real processes, in-process, without needing real sockets:
each side's `on_signal` closure pushes directly into the other side's
inbox. Real ICE candidate gathering/handshake still takes real wall-clock
time even on loopback, so `connect_pair` polls to `Connected` with a
generous timeout rather than expecting instant success.
