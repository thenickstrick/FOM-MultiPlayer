// Not built by cargo. A standalone reference tool: prints the real
// sizeof/alignof/offsetof values for the structs bound in src/lib.rs,
// compiled against the actual pinned GameNetworkingSockets header rather
// than derived by hand. Re-run this after re-pinning the submodule and
// update tests/layout.rs with whatever it prints.
//
//   c++ -std=c++17 -I third_party/GameNetworkingSockets/include/steam \
//       -o /tmp/probe crates/gns-sys/tests/layout_probe.cpp && /tmp/probe

#include <cstddef>
#include <cstdio>
#include "steamnetworkingtypes.h"

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
    return 0;
}
