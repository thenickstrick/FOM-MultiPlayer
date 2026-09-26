//! Struct-layout smoke test: verifies every hand-bound struct's ABI against
//! the real pinned header. See ../README.md for what this checks and how
//! to re-capture the numbers after a re-pin.

use gns_sys::{SteamNetworkingIdentity, SteamNetworkingMessage_t};
use std::mem::{align_of, offset_of, size_of};

#[test]
fn steam_networking_identity_layout() {
    assert_eq!(size_of::<SteamNetworkingIdentity>(), 136);
    assert_eq!(align_of::<SteamNetworkingIdentity>(), 1);
}

#[test]
fn steam_networking_message_layout() {
    assert_eq!(size_of::<SteamNetworkingMessage_t>(), 216);
    assert_eq!(align_of::<SteamNetworkingMessage_t>(), 8);

    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_pData), 0);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_cbSize), 8);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_conn), 12);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_identityPeer), 16);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_nConnUserData), 152);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_pfnFreeData), 176);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_pfnRelease), 184);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_nChannel), 192);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_nUserData), 200);
    assert_eq!(offset_of!(SteamNetworkingMessage_t, m_idxLane), 208);
}
