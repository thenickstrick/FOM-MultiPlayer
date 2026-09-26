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

// ---- Opaque interface handle -----------------------------------------------
// Never held except as a pointer (see README.md); empty enum is the
// standard Rust idiom for that.
pub enum ISteamNetworkingSockets {}

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
}
