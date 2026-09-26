//! Hand-written FFI bindings to GameNetworkingSockets' flat C API. See
//! `README.md` for why hand-written (not bindgen) and the struct-layout
//! verification discipline.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};

// ---- Scalar typedefs (steamnetworkingtypes.h / steamtypes.h) --------------

pub type HSteamNetConnection = u32;
pub type HSteamListenSocket = u32;
pub type HSteamNetPollGroup = u32;
pub type SteamNetworkingMicroseconds = i64;
pub type SteamNetworkingPOPID = u32;
pub type EResult = c_int;
pub const K_ERESULT_OK: EResult = 1;
pub type ESteamNetworkingConnectionState = c_int;
pub type ESteamNetworkingConfigValue = c_int;
pub type ESteamNetworkingConfigDataType = c_int;

// ---- ESteamNetworkingConnectionState (steamnetworkingtypes.h:678-747) -----

pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_NONE: ESteamNetworkingConnectionState = 0;
pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTING: ESteamNetworkingConnectionState = 1;
pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_FINDING_ROUTE: ESteamNetworkingConnectionState = 2;
pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_CONNECTED: ESteamNetworkingConnectionState = 3;
pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_CLOSED_BY_PEER: ESteamNetworkingConnectionState = 4;
pub const K_ESTEAMNETWORKINGCONNECTIONSTATE_PROBLEM_DETECTED_LOCALLY: ESteamNetworkingConnectionState = 5;

// ---- ESteamNetworkingConfigValue / DataType (steamnetworkingtypes.h) ------
// Only the entries this workspace sets are bound (see README.md).

/// [connection int32] Do the "symmetric connection initiation" mode, in
/// which either peer may act as the "client" or "server" (steamnetworkingtypes.h:1373).
pub const K_ESTEAMNETWORKINGCONFIG_SYMMETRIC_CONNECT: ESteamNetworkingConfigValue = 37;

pub const K_ESTEAMNETWORKINGCONFIG_INT32: ESteamNetworkingConfigDataType = 1;

/// `k_cchMaxSteamNetworkingErrMsg` (steamnetworkingtypes.h:88).
pub const K_CCH_MAX_STEAMNETWORKING_ERR_MSG: usize = 1024;
/// `typedef char SteamNetworkingErrMsg[k_cchMaxSteamNetworkingErrMsg]`.
pub type SteamNetworkingErrMsg = [c_char; K_CCH_MAX_STEAMNETWORKING_ERR_MSG];

// ---- Send flags (steamnetworkingtypes.h) -----------------------------------

pub const K_N_STEAMNETWORKINGSEND_UNRELIABLE: c_int = 0;
pub const K_N_STEAMNETWORKINGSEND_NO_NAGLE: c_int = 1;
pub const K_N_STEAMNETWORKINGSEND_NO_DELAY: c_int = 4;
pub const K_N_STEAMNETWORKINGSEND_RELIABLE: c_int = 8;

// ---- SteamNetworkingIdentity (steamnetworkingtypes.h:262-347) -------------
// Layout and packed(1) rationale: see README.md.
#[repr(C, packed(1))]
#[derive(Clone, Copy)]
pub struct SteamNetworkingIdentity {
    pub m_eType: c_int,
    pub m_cbSize: c_int,
    pub m_union: [u32; 32],
}

impl SteamNetworkingIdentity {
    pub fn invalid() -> Self {
        SteamNetworkingIdentity {
            m_eType: 0, // k_ESteamNetworkingIdentityType_Invalid
            m_cbSize: 0,
            m_union: [0; 32],
        }
    }
}

// ---- SteamNetworkingMessage_t (steamnetworkingtypes.h:849-928) ------------
// Field order and Release() rationale: see README.md.
#[repr(C)]
pub struct SteamNetworkingMessage_t {
    pub m_pData: *mut c_void,
    pub m_cbSize: c_int,
    pub m_conn: HSteamNetConnection,
    pub m_identityPeer: SteamNetworkingIdentity,
    pub m_nConnUserData: i64,
    pub m_usecTimeReceived: SteamNetworkingMicroseconds,
    pub m_nMessageNumber: i64,
    pub m_pfnFreeData: Option<unsafe extern "C" fn(*mut SteamNetworkingMessage_t)>,
    pub m_pfnRelease: Option<unsafe extern "C" fn(*mut SteamNetworkingMessage_t)>,
    pub m_nChannel: c_int,
    pub m_nFlags: c_int,
    pub m_nUserData: i64,
    pub m_idxLane: u16,
    pub _pad1__: u16,
}

impl SteamNetworkingMessage_t {
    /// Payload as a byte slice. Safety: the pointer must still be valid,
    /// i.e. this must be called before `release()`.
    pub unsafe fn data(&self) -> &[u8] {
        std::slice::from_raw_parts(self.m_pData as *const u8, self.m_cbSize as usize)
    }

    /// Equivalent of the C++ inline `Release()` — every message returned by
    /// the API must have this called on it exactly once.
    pub unsafe fn release(msg: *mut SteamNetworkingMessage_t) {
        if let Some(f) = (*msg).m_pfnRelease {
            f(msg);
        }
    }
}

// ---- SteamNetworkingIPAddr (steamnetworkingtypes.h:203-255) ---------------
// Opaque correctly-sized blob only; see README.md.
#[repr(C, packed(1))]
#[derive(Clone, Copy)]
pub struct SteamNetworkingIPAddr {
    pub m_ipv6: [u8; 16],
    pub m_port: u16,
}

impl SteamNetworkingIPAddr {
    pub fn zeroed() -> Self {
        SteamNetworkingIPAddr {
            m_ipv6: [0; 16],
            m_port: 0,
        }
    }
}

// ---- SteamNetConnectionInfo_t (steamnetworkingtypes.h:678-729) ------------
// Field-for-field, packed(1), offsets verified in tests/layout.rs (README.md).
#[repr(C, packed(1))]
#[derive(Clone, Copy)]
pub struct SteamNetConnectionInfo_t {
    pub m_identityRemote: SteamNetworkingIdentity,
    pub m_nUserData: i64,
    pub m_hListenSocket: HSteamListenSocket,
    pub m_addrRemote: SteamNetworkingIPAddr,
    pub m__pad1: u16,
    pub m_idPOPRemote: SteamNetworkingPOPID,
    pub m_idPOPRelay: SteamNetworkingPOPID,
    pub m_eState: ESteamNetworkingConnectionState,
    pub m_eEndReason: c_int,
    pub m_szEndDebug: [c_char; 128],
    pub m_szConnectionDescription: [c_char; 128],
    pub m_nFlags: c_int,
    pub reserved: [u32; 63],
}

impl SteamNetConnectionInfo_t {
    /// Only meaningful as a scratch buffer for `GetConnectionInfo` to fill.
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// ---- SteamNetworkingConfigValue_t (steamnetworkingtypes.h:1747-1798) ------
// Plain #[repr(C)] reproduces the real layout exactly; see README.md.
#[repr(C)]
#[derive(Clone, Copy)]
pub union SteamNetworkingConfigValue {
    pub m_int32: i32,
    pub m_int64: i64,
    pub m_float: f32,
    pub m_string: *const c_char,
    pub m_ptr: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SteamNetworkingConfigValue_t {
    pub m_eValue: ESteamNetworkingConfigValue,
    pub m_eDataType: ESteamNetworkingConfigDataType,
    pub m_val: SteamNetworkingConfigValue,
}

impl SteamNetworkingConfigValue_t {
    pub fn int32(value: ESteamNetworkingConfigValue, data: i32) -> Self {
        SteamNetworkingConfigValue_t {
            m_eValue: value,
            m_eDataType: K_ESTEAMNETWORKINGCONFIG_INT32,
            m_val: SteamNetworkingConfigValue { m_int32: data },
        }
    }
}

// ---- Opaque interface handle -----------------------------------------------
// Never held except as a pointer (see README.md); empty enum is the
// standard Rust idiom for that.
pub enum ISteamNetworkingSockets {}

/// Same idiom as `ISteamNetworkingSockets`: only ever held/passed as a
/// pointer, obtained from and consumed by `SteamAPI_ISteamNetworkingSockets_CreateCustomSignaling`.
pub enum ISteamNetworkingConnectionSignaling {}

// ---- Custom signaling plain-C callbacks (steamnetworkingcustomsignaling.h,
// steamnetworkingsockets_flat.h:154-176) — a plain-C bridge, not a
// hand-rolled vtable; see README.md for why and the `const T&`-as-`T*` note.
pub type FSteamNetworkingSocketsCustomSignaling_SendSignal = unsafe extern "C" fn(
    ctx: *mut c_void,
    hConn: HSteamNetConnection,
    info: *const SteamNetConnectionInfo_t,
    pMsg: *const c_void,
    cbMsg: c_int,
) -> bool;

pub type FSteamNetworkingSocketsCustomSignaling_Release = unsafe extern "C" fn(ctx: *mut c_void);

pub type FSteamNetworkingCustomSignalingRecvContext_OnConnectRequest = unsafe extern "C" fn(
    ctx: *mut c_void,
    hConn: HSteamNetConnection,
    identityPeer: *const SteamNetworkingIdentity,
    nLocalVirtualPort: c_int,
) -> *mut ISteamNetworkingConnectionSignaling;

pub type FSteamNetworkingCustomSignalingRecvContext_SendRejectionSignal = unsafe extern "C" fn(
    ctx: *mut c_void,
    identityPeer: *const SteamNetworkingIdentity,
    pMsg: *const c_void,
    cbMsg: c_int,
);

// ---- Flat API (steamnetworkingsockets_flat.h) ------------------------------

extern "C" {
    // steamnetworkingsockets.h — standalone (non-Steam) lifecycle.
    pub fn GameNetworkingSockets_Init(
        pIdentity: *const SteamNetworkingIdentity,
        errMsg: *mut SteamNetworkingErrMsg,
    ) -> bool;
    pub fn GameNetworkingSockets_Kill();

    // steamnetworkingsockets_flat.h — ISteamNetworkingSockets.
    pub fn SteamAPI_SteamNetworkingSockets_v009() -> *mut ISteamNetworkingSockets;

    pub fn SteamAPI_ISteamNetworkingSockets_CreateSocketPair(
        self_: *mut ISteamNetworkingSockets,
        pOutConnection1: *mut HSteamNetConnection,
        pOutConnection2: *mut HSteamNetConnection,
        bUseNetworkLoopback: bool,
        pIdentity1: *const SteamNetworkingIdentity,
        pIdentity2: *const SteamNetworkingIdentity,
    ) -> bool;

    pub fn SteamAPI_ISteamNetworkingSockets_SendMessageToConnection(
        self_: *mut ISteamNetworkingSockets,
        hConn: HSteamNetConnection,
        pData: *const c_void,
        cbData: u32,
        nSendFlags: c_int,
        pOutMessageNumber: *mut i64,
    ) -> EResult;

    pub fn SteamAPI_ISteamNetworkingSockets_ReceiveMessagesOnConnection(
        self_: *mut ISteamNetworkingSockets,
        hConn: HSteamNetConnection,
        ppOutMessages: *mut *mut SteamNetworkingMessage_t,
        nMaxMessages: c_int,
    ) -> c_int;

    pub fn SteamAPI_ISteamNetworkingSockets_CloseConnection(
        self_: *mut ISteamNetworkingSockets,
        hPeer: HSteamNetConnection,
        nReason: c_int,
        pszDebug: *const c_char,
        bEnableLinger: bool,
    ) -> bool;

    pub fn SteamAPI_ISteamNetworkingSockets_RunCallbacks(self_: *mut ISteamNetworkingSockets);

    pub fn SteamAPI_ISteamNetworkingSockets_GetConnectionInfo(
        self_: *mut ISteamNetworkingSockets,
        hConn: HSteamNetConnection,
        pInfo: *mut SteamNetConnectionInfo_t,
    ) -> bool;

    pub fn SteamAPI_ISteamNetworkingSockets_AcceptConnection(
        self_: *mut ISteamNetworkingSockets,
        hConn: HSteamNetConnection,
    ) -> EResult;

    // steamnetworkingsockets_flat.h:154-176 — plain-C custom signaling bridge.
    pub fn SteamAPI_ISteamNetworkingSockets_CreateCustomSignaling(
        ctx: *mut c_void,
        fnSendSignal: FSteamNetworkingSocketsCustomSignaling_SendSignal,
        fnRelease: Option<FSteamNetworkingSocketsCustomSignaling_Release>,
    ) -> *mut ISteamNetworkingConnectionSignaling;

    pub fn SteamAPI_ISteamNetworkingSockets_ConnectP2PCustomSignaling(
        self_: *mut ISteamNetworkingSockets,
        pSignaling: *mut ISteamNetworkingConnectionSignaling,
        pPeerIdentity: *const SteamNetworkingIdentity,
        nRemoteVirtualPort: c_int,
        nOptions: c_int,
        pOptions: *const SteamNetworkingConfigValue_t,
    ) -> HSteamNetConnection;

    pub fn SteamAPI_ISteamNetworkingSockets_ReceivedP2PCustomSignal2(
        self_: *mut ISteamNetworkingSockets,
        pMsg: *const c_void,
        cbMsg: c_int,
        ctx: *mut c_void,
        fnOnConnectRequest: FSteamNetworkingCustomSignalingRecvContext_OnConnectRequest,
        fnSendRejectionSignal: Option<FSteamNetworkingCustomSignalingRecvContext_SendRejectionSignal>,
    ) -> bool;
}
