// Not built by cargo. Standalone reference tool; see ../README.md.

#include <cstddef>
#include <cstdio>
#include "steamnetworkingtypes.h"
#include "isteamnetworkingsockets.h"

int main() {
    printf("SteamNetworkingIdentity: size=%zu align=%zu\n",
        sizeof(SteamNetworkingIdentity), alignof(SteamNetworkingIdentity));
    printf("SteamNetworkingMessage_t: size=%zu align=%zu\n",
        sizeof(SteamNetworkingMessage_t), alignof(SteamNetworkingMessage_t));
    printf("offsetof m_pData=%zu m_cbSize=%zu m_conn=%zu m_identityPeer=%zu "
           "m_nConnUserData=%zu m_pfnFreeData=%zu m_pfnRelease=%zu "
           "m_nChannel=%zu m_nUserData=%zu m_idxLane=%zu\n",
        offsetof(SteamNetworkingMessage_t, m_pData),
        offsetof(SteamNetworkingMessage_t, m_cbSize),
        offsetof(SteamNetworkingMessage_t, m_conn),
        offsetof(SteamNetworkingMessage_t, m_identityPeer),
        offsetof(SteamNetworkingMessage_t, m_nConnUserData),
        offsetof(SteamNetworkingMessage_t, m_pfnFreeData),
        offsetof(SteamNetworkingMessage_t, m_pfnRelease),
        offsetof(SteamNetworkingMessage_t, m_nChannel),
        offsetof(SteamNetworkingMessage_t, m_nUserData),
        offsetof(SteamNetworkingMessage_t, m_idxLane));

    printf("SteamNetworkingIPAddr: size=%zu align=%zu\n",
        sizeof(SteamNetworkingIPAddr), alignof(SteamNetworkingIPAddr));
    printf("offsetof m_ipv6=%zu m_port=%zu\n",
        offsetof(SteamNetworkingIPAddr, m_ipv6),
        offsetof(SteamNetworkingIPAddr, m_port));

    printf("SteamNetConnectionInfo_t: size=%zu align=%zu\n",
        sizeof(SteamNetConnectionInfo_t), alignof(SteamNetConnectionInfo_t));
    printf("offsetof m_identityRemote=%zu m_nUserData=%zu m_hListenSocket=%zu "
           "m_addrRemote=%zu m_idPOPRemote=%zu m_idPOPRelay=%zu m_eState=%zu "
           "m_eEndReason=%zu m_szEndDebug=%zu m_szConnectionDescription=%zu "
           "m_nFlags=%zu\n",
        offsetof(SteamNetConnectionInfo_t, m_identityRemote),
        offsetof(SteamNetConnectionInfo_t, m_nUserData),
        offsetof(SteamNetConnectionInfo_t, m_hListenSocket),
        offsetof(SteamNetConnectionInfo_t, m_addrRemote),
        offsetof(SteamNetConnectionInfo_t, m_idPOPRemote),
        offsetof(SteamNetConnectionInfo_t, m_idPOPRelay),
        offsetof(SteamNetConnectionInfo_t, m_eState),
        offsetof(SteamNetConnectionInfo_t, m_eEndReason),
        offsetof(SteamNetConnectionInfo_t, m_szEndDebug),
        offsetof(SteamNetConnectionInfo_t, m_szConnectionDescription),
        offsetof(SteamNetConnectionInfo_t, m_nFlags));

    printf("SteamNetworkingConfigValue_t: size=%zu align=%zu\n",
        sizeof(SteamNetworkingConfigValue_t), alignof(SteamNetworkingConfigValue_t));
    printf("offsetof m_eValue=%zu m_eDataType=%zu m_val=%zu\n",
        offsetof(SteamNetworkingConfigValue_t, m_eValue),
        offsetof(SteamNetworkingConfigValue_t, m_eDataType),
        offsetof(SteamNetworkingConfigValue_t, m_val));

    printf("SteamNetConnectionStatusChangedCallback_t: size=%zu align=%zu\n",
        sizeof(SteamNetConnectionStatusChangedCallback_t), alignof(SteamNetConnectionStatusChangedCallback_t));
    printf("offsetof m_hConn=%zu m_info=%zu m_eOldState=%zu\n",
        offsetof(SteamNetConnectionStatusChangedCallback_t, m_hConn),
        offsetof(SteamNetConnectionStatusChangedCallback_t, m_info),
        offsetof(SteamNetConnectionStatusChangedCallback_t, m_eOldState));

    return 0;
}
