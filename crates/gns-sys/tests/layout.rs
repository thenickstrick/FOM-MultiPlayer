//! Struct-layout smoke test: verifies every hand-bound struct's ABI against
//! the real pinned header. See ../README.md for what this checks and how
//! to re-capture the numbers after a re-pin.

use gns_sys::{
    SteamNetConnectionInfo_t, SteamNetworkingConfigValue_t, SteamNetworkingIPAddr,
    SteamNetworkingIdentity, SteamNetworkingMessage_t,
};
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

#[test]
fn steam_networking_ip_addr_layout() {
    assert_eq!(size_of::<SteamNetworkingIPAddr>(), 18);
    assert_eq!(align_of::<SteamNetworkingIPAddr>(), 1);
    assert_eq!(offset_of!(SteamNetworkingIPAddr, m_ipv6), 0);
    assert_eq!(offset_of!(SteamNetworkingIPAddr, m_port), 16);
}

#[test]
fn steam_net_connection_info_layout() {
    assert_eq!(size_of::<SteamNetConnectionInfo_t>(), 696);
    assert_eq!(align_of::<SteamNetConnectionInfo_t>(), 1);

    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_identityRemote), 0);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_nUserData), 136);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_hListenSocket), 144);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_addrRemote), 148);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_idPOPRemote), 168);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_idPOPRelay), 172);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_eState), 176);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_eEndReason), 180);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_szEndDebug), 184);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_szConnectionDescription), 312);
    assert_eq!(offset_of!(SteamNetConnectionInfo_t, m_nFlags), 440);
}

#[test]
fn steam_networking_config_value_layout() {
    assert_eq!(size_of::<SteamNetworkingConfigValue_t>(), 16);
    assert_eq!(align_of::<SteamNetworkingConfigValue_t>(), 8);
    assert_eq!(offset_of!(SteamNetworkingConfigValue_t, m_eValue), 0);
    assert_eq!(offset_of!(SteamNetworkingConfigValue_t, m_eDataType), 4);
    assert_eq!(offset_of!(SteamNetworkingConfigValue_t, m_val), 8);
}
