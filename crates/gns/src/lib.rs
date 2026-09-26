//! Safe Rust wrapper over `gns-sys`: connection lifecycle, custom P2P
//! signaling, and message send/receive. See `README.md` for the design.

use gns_sys::{
    ESteamNetworkingConnectionState, HSteamNetConnection, ISteamNetworkingConnectionSignaling,
    ISteamNetworkingSockets, SteamNetConnectionInfo_t, SteamNetworkingConfigValue_t,
    SteamNetworkingErrMsg, K_CCH_MAX_STEAMNETWORKING_ERR_MSG, K_ERESULT_OK,
    K_ESTEAMNETWORKINGCONFIG_SYMMETRIC_CONNECT, K_ESTEAMNETWORKINGCONNECTIONSTATE_CLOSED_BY_PEER,
    K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTED, K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTING,
    K_ESTEAMNETWORKINGCONNECTIONSTATE_FINDING_ROUTE, K_ESTEAMNETWORKINGCONNECTIONSTATE_NONE,
    K_ESTEAMNETWORKINGCONNECTIONSTATE_PROBLEM_DETECTED_LOCALLY, K_N_STEAMNETWORKINGSEND_RELIABLE,
    K_N_STEAMNETWORKINGSEND_UNRELIABLE,
};
use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::Once;

static INIT: Once = Once::new();

/// A boxed opaque-blob sender for the peer's rendezvous channel. GNS may
/// invoke this from its own thread at any time; see README.md.
pub type SignalSender = Box<dyn FnMut(&[u8]) -> bool + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GnsError(pub c_int);

impl std::fmt::Display for GnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GNS call failed (EResult={})", self.0)
    }
}

impl std::error::Error for GnsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    None,
    Connecting,
    FindingRoute,
    Connected,
    ClosedByPeer,
    ProblemDetectedLocally,
    /// A value this crate doesn't have a variant for (internal-only states
    /// like FinWait/Linger/Dead are never returned by the public API, so
    /// this should not occur in practice).
    Other(ESteamNetworkingConnectionState),
}

impl ConnectionState {
    fn from_raw(raw: ESteamNetworkingConnectionState) -> Self {
        match raw {
            K_ESTEAMNETWORKINGCONNECTIONSTATE_NONE => ConnectionState::None,
            K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTING => ConnectionState::Connecting,
            K_ESTEAMNETWORKINGCONNECTIONSTATE_FINDING_ROUTE => ConnectionState::FindingRoute,
            K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTED => ConnectionState::Connected,
            K_ESTEAMNETWORKINGCONNECTIONSTATE_CLOSED_BY_PEER => ConnectionState::ClosedByPeer,
            K_ESTEAMNETWORKINGCONNECTIONSTATE_PROBLEM_DETECTED_LOCALLY => {
                ConnectionState::ProblemDetectedLocally
            }
            other => ConnectionState::Other(other),
        }
    }
}

pub struct Gns {
    interface: *mut ISteamNetworkingSockets,
}

// The underlying C++ interface's methods are documented as safe to call
// from any thread concurrently; we only ever hand out `&Gns`, never `&mut`.
unsafe impl Send for Gns {}
unsafe impl Sync for Gns {}

impl Gns {
    /// Initializes the standalone (non-Steam) GNS library, process-wide.
    /// Safe to call more than once (e.g. from independent tests in the same
    /// binary); only the first call does the real work.
    pub fn init() -> Result<Self, String> {
        let mut init_ok = true;
        let mut err_buf: SteamNetworkingErrMsg = [0; K_CCH_MAX_STEAMNETWORKING_ERR_MSG];
        INIT.call_once(|| {
            init_ok = unsafe { gns_sys::GameNetworkingSockets_Init(std::ptr::null(), &mut err_buf) };
        });
        if !init_ok {
            let end = err_buf.iter().position(|&b| b == 0).unwrap_or(err_buf.len());
            let bytes: Vec<u8> = err_buf[..end].iter().map(|&c| c as u8).collect();
            return Err(String::from_utf8_lossy(&bytes).into_owned());
        }
        let interface = unsafe { gns_sys::SteamAPI_SteamNetworkingSockets_v009() };
        assert!(
            !interface.is_null(),
            "SteamAPI_SteamNetworkingSockets_v009 returned null after successful Init"
        );
        Ok(Gns { interface })
    }

    /// Pumps GNS's internal callback queue; call regularly (see README.md).
    pub fn run_callbacks(&self) {
        unsafe { gns_sys::SteamAPI_ISteamNetworkingSockets_RunCallbacks(self.interface) }
    }

    /// Initiates a P2P connection using a caller-supplied signaling
    /// channel; see README.md for `symmetric`'s caveat.
    pub fn connect_p2p_custom_signaling(
        &self,
        on_signal: impl FnMut(&[u8]) -> bool + Send + 'static,
        symmetric: bool,
    ) -> GnsConnection<'_> {
        let ctx = signal_sender_into_ctx(Box::new(on_signal));
        let signaling = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_CreateCustomSignaling(
                ctx,
                send_signal_trampoline,
                Some(release_trampoline),
            )
        };

        let options = symmetric_connect_options(symmetric);
        let handle = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_ConnectP2PCustomSignaling(
                self.interface,
                signaling,
                std::ptr::null(), // peer identity: unknown/unneeded, confirmed via rendezvous
                0,                // remote virtual port
                options.len() as c_int,
                if options.is_empty() { std::ptr::null() } else { options.as_ptr() },
            )
        };

        GnsConnection { gns: self, handle, closed: false }
    }

    /// Feeds a rendezvous blob from the peer into GNS. Returns the accepted
    /// `GnsConnection` if it completed a new inbound one; see README.md.
    pub fn receive_signal(
        &self,
        data: &[u8],
        mut on_connect_request: impl FnMut(HSteamNetConnection) -> Option<SignalSender>,
    ) -> Option<GnsConnection<'_>> {
        let mut accepted_handle: Option<HSteamNetConnection> = None;
        let mut recv_ctx = RecvCtx {
            on_connect_request: &mut on_connect_request,
            accepted_handle: &mut accepted_handle,
        };

        unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_ReceivedP2PCustomSignal2(
                self.interface,
                data.as_ptr() as *const c_void,
                data.len() as c_int,
                &mut recv_ctx as *mut RecvCtx as *mut c_void,
                on_connect_request_trampoline,
                None, // rejection signals: nothing to notify, see OnConnectRequest doc above
            );
        }

        accepted_handle.map(|handle| GnsConnection { gns: self, handle, closed: false })
    }
}

fn symmetric_connect_options(symmetric: bool) -> Vec<SteamNetworkingConfigValue_t> {
    if symmetric {
        vec![SteamNetworkingConfigValue_t::int32(K_ESTEAMNETWORKINGCONFIG_SYMMETRIC_CONNECT, 1)]
    } else {
        Vec::new()
    }
}

fn signal_sender_into_ctx(sender: SignalSender) -> *mut c_void {
    // Double-box: `Box<dyn Trait>` is a fat pointer, which doesn't fit in a
    // `*mut c_void`. Boxing it again gives a thin pointer to that fat
    // pointer, which does. Reclaimed in `release_trampoline`.
    Box::into_raw(Box::new(sender)) as *mut c_void
}

unsafe extern "C" fn send_signal_trampoline(
    ctx: *mut c_void,
    _hconn: HSteamNetConnection,
    _info: *const SteamNetConnectionInfo_t,
    msg: *const c_void,
    cb_msg: c_int,
) -> bool {
    let sender = &mut *(ctx as *mut SignalSender);
    let bytes = std::slice::from_raw_parts(msg as *const u8, cb_msg as usize);
    sender(bytes)
}

unsafe extern "C" fn release_trampoline(ctx: *mut c_void) {
    drop(Box::from_raw(ctx as *mut SignalSender));
}

struct RecvCtx<'a> {
    on_connect_request: &'a mut dyn FnMut(HSteamNetConnection) -> Option<SignalSender>,
    accepted_handle: &'a mut Option<HSteamNetConnection>,
}

unsafe extern "C" fn on_connect_request_trampoline(
    ctx: *mut c_void,
    hconn: HSteamNetConnection,
    _identity_peer: *const gns_sys::SteamNetworkingIdentity,
    _local_virtual_port: c_int,
) -> *mut ISteamNetworkingConnectionSignaling {
    let recv_ctx = &mut *(ctx as *mut RecvCtx);
    match (recv_ctx.on_connect_request)(hconn) {
        Some(sender) => {
            *recv_ctx.accepted_handle = Some(hconn);
            let ctx = signal_sender_into_ctx(sender);
            gns_sys::SteamAPI_ISteamNetworkingSockets_CreateCustomSignaling(
                ctx,
                send_signal_trampoline,
                Some(release_trampoline),
            )
        }
        None => std::ptr::null_mut(),
    }
}

pub struct GnsConnection<'g> {
    gns: &'g Gns,
    handle: HSteamNetConnection,
    closed: bool,
}

impl<'g> GnsConnection<'g> {
    pub fn handle(&self) -> HSteamNetConnection {
        self.handle
    }

    /// Accepts an inbound connection. Only meaningful while `state()` is
    /// `Connecting`; per GNS's docs, an already-accepted or symmetric
    /// implicitly-accepted connection tolerates a redundant call.
    pub fn accept(&self) -> Result<(), GnsError> {
        let result = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_AcceptConnection(self.gns.interface, self.handle)
        };
        if result == K_ERESULT_OK {
            Ok(())
        } else {
            Err(GnsError(result))
        }
    }

    pub fn state(&self) -> ConnectionState {
        let mut info = SteamNetConnectionInfo_t::zeroed();
        let ok = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_GetConnectionInfo(
                self.gns.interface,
                self.handle,
                &mut info,
            )
        };
        if !ok {
            return ConnectionState::None;
        }
        ConnectionState::from_raw(info.m_eState)
    }

    /// End-of-connection reason code and human-readable debug string. Only
    /// meaningful once `state()` is no longer `Connecting`/`FindingRoute`;
    /// see `SteamNetConnectionInfo_t::m_eEndReason`/`m_szEndDebug`.
    pub fn end_reason_debug(&self) -> (i32, String) {
        let mut info = SteamNetConnectionInfo_t::zeroed();
        unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_GetConnectionInfo(
                self.gns.interface,
                self.handle,
                &mut info,
            );
        }
        let debug = info
            .m_szEndDebug
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8 as char)
            .collect();
        (info.m_eEndReason, debug)
    }

    pub fn send_reliable(&self, data: &[u8]) -> Result<i64, GnsError> {
        self.send(data, K_N_STEAMNETWORKINGSEND_RELIABLE)
    }

    pub fn send_unreliable(&self, data: &[u8]) -> Result<i64, GnsError> {
        self.send(data, K_N_STEAMNETWORKINGSEND_UNRELIABLE)
    }

    fn send(&self, data: &[u8], flags: c_int) -> Result<i64, GnsError> {
        let mut message_number = 0i64;
        let result = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_SendMessageToConnection(
                self.gns.interface,
                self.handle,
                data.as_ptr() as *const c_void,
                data.len() as u32,
                flags,
                &mut message_number,
            )
        };
        if result == K_ERESULT_OK {
            Ok(message_number)
        } else {
            Err(GnsError(result))
        }
    }

    /// Drains up to `max` pending inbound messages, oldest first.
    pub fn poll_messages(&self, max: usize) -> Vec<Vec<u8>> {
        let mut raw = vec![std::ptr::null_mut(); max];
        let count = unsafe {
            gns_sys::SteamAPI_ISteamNetworkingSockets_ReceiveMessagesOnConnection(
                self.gns.interface,
                self.handle,
                raw.as_mut_ptr(),
                max as c_int,
            )
        };
        let mut out = Vec::with_capacity(count.max(0) as usize);
        for &msg in &raw[..count.max(0) as usize] {
            unsafe {
                out.push((*msg).data().to_vec());
                gns_sys::SteamNetworkingMessage_t::release(msg);
            }
        }
        out
    }

    /// Closes the connection now, rather than waiting for `Drop`, so the
    /// caller can distinguish "already closed" if that matters to them.
    pub fn close(mut self) {
        self.close_inner();
    }

    fn close_inner(&mut self) {
        if !self.closed {
            unsafe {
                gns_sys::SteamAPI_ISteamNetworkingSockets_CloseConnection(
                    self.gns.interface,
                    self.handle,
                    0,
                    std::ptr::null(),
                    false,
                );
            }
            self.closed = true;
        }
    }
}

impl<'g> Drop for GnsConnection<'g> {
    fn drop(&mut self) {
        self.close_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    type SignalInbox = Arc<Mutex<Vec<Vec<u8>>>>;

    /// In-process "signaling relay" simulating mp-relay's real transport
    /// (see README.md) without needing real sockets here.
    fn wire_loopback_signaling() -> (SignalInbox, SignalInbox) {
        (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())))
    }

    fn pump_signals<'g>(
        gns: &'g Gns,
        inbox: &SignalInbox,
        accept_new: &mut dyn FnMut(HSteamNetConnection) -> Option<SignalSender>,
    ) -> Option<GnsConnection<'g>> {
        let pending: Vec<Vec<u8>> = std::mem::take(&mut *inbox.lock().unwrap());
        let mut accepted = None;
        for signal in pending {
            if let Some(conn) = gns.receive_signal(&signal, &mut *accept_new) {
                accepted = Some(conn);
            }
        }
        accepted
    }

    /// Connects two in-process peers and drives both to `Connected` or
    /// panics after a timeout (see README.md on why it's generous).
    fn connect_pair(gns: &Gns, symmetric: bool) -> (GnsConnection<'_>, GnsConnection<'_>) {
        let (a_inbox, b_inbox) = wire_loopback_signaling();

        // Side A initiates. Its outbound signals land in B's inbox.
        let b_inbox_for_a = b_inbox.clone();
        let conn_a = gns.connect_p2p_custom_signaling(
            move |blob: &[u8]| {
                b_inbox_for_a.lock().unwrap().push(blob.to_vec());
                true
            },
            symmetric,
        );

        // Side B doesn't know about the connection yet; it learns about it
        // (and gets its own signal sender wired up) the first time it
        // processes a signal that represents a new inbound request.
        let a_inbox_for_b = a_inbox.clone();
        let mut accept_b = move |_hconn: HSteamNetConnection| -> Option<SignalSender> {
            let a_inbox_for_b = a_inbox_for_b.clone();
            Some(Box::new(move |blob: &[u8]| {
                a_inbox_for_b.lock().unwrap().push(blob.to_vec());
                true
            }))
        };

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut conn_b: Option<GnsConnection> = None;
        loop {
            gns.run_callbacks();

            // A's outbound signals land in b_inbox; B processes them and, on
            // the first one that represents a new request, gets a
            // connection.
            if let Some(conn) = pump_signals(gns, &b_inbox, &mut accept_b) {
                conn.accept().expect("accept inbound connection");
                conn_b = Some(conn);
            }
            // B's replies land in a_inbox; A already has its connection
            // object from initiating, so these are always follow-up
            // signals for it, never a new request.
            let _ = pump_signals(gns, &a_inbox, &mut |_| None);

            if conn_a.state() == ConnectionState::Connected
                && conn_b.as_ref().map(|c| c.state()) == Some(ConnectionState::Connected)
            {
                break;
            }
            if Instant::now() > deadline {
                panic!(
                    "connection did not reach Connected within timeout (a={:?} {:?}, b={:?})",
                    conn_a.state(),
                    conn_a.end_reason_debug(),
                    conn_b.as_ref().map(|c| (c.state(), c.end_reason_debug()))
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        (conn_a, conn_b.unwrap())
    }

    fn assert_bidirectional_exchange(gns: &Gns, conn_a: &GnsConnection, conn_b: &GnsConnection) {
        conn_a.send_reliable(b"hello from a").unwrap();
        conn_b.send_reliable(b"hello from b").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got_a = false;
        let mut got_b = false;
        while (!got_a || !got_b) && Instant::now() < deadline {
            gns.run_callbacks();
            for msg in conn_b.poll_messages(8) {
                if msg == b"hello from a" {
                    got_a = true;
                }
            }
            for msg in conn_a.poll_messages(8) {
                if msg == b"hello from b" {
                    got_b = true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(got_a, "connection B never received A's message");
        assert!(got_b, "connection A never received B's message");
    }

    #[test]
    fn connects_and_exchanges_messages_via_custom_signaling() {
        let gns = Gns::init().expect("GNS init");
        let (conn_a, conn_b) = connect_pair(&gns, false);
        assert_bidirectional_exchange(&gns, &conn_a, &conn_b);
    }
}
