//! Loopback FFI smoke test: init, create a connected socket pair (GNS's
//! own built-in testing primitive, no network/signaling involved), send
//! one way, receive on the other end.

use gns_sys::*;
use std::ffi::c_void;
use std::ptr;

#[test]
fn loopback_socket_pair_send_receive() {
    unsafe {
        let mut err_msg: SteamNetworkingErrMsg = [0; K_CCH_MAX_STEAMNETWORKING_ERR_MSG];
        let ok = GameNetworkingSockets_Init(ptr::null(), &mut err_msg);
        assert!(ok, "GameNetworkingSockets_Init failed: {:?}", cstr(&err_msg));

        let sockets = SteamAPI_SteamNetworkingSockets_v009();
        assert!(!sockets.is_null(), "SteamAPI_SteamNetworkingSockets_v009 returned null");

        let mut conn_a: HSteamNetConnection = 0;
        let mut conn_b: HSteamNetConnection = 0;
        let paired = SteamAPI_ISteamNetworkingSockets_CreateSocketPair(
            sockets,
            &mut conn_a,
            &mut conn_b,
            false, // bUseNetworkLoopback: false = in-process, no real sockets/lag/loss
            ptr::null(),
            ptr::null(),
        );
        assert!(paired, "CreateSocketPair failed");

        let payload = b"hello from a to b";
        let result = SteamAPI_ISteamNetworkingSockets_SendMessageToConnection(
            sockets,
            conn_a,
            payload.as_ptr() as *const c_void,
            payload.len() as u32,
            K_N_STEAMNETWORKINGSEND_RELIABLE,
            ptr::null_mut(),
        );
        assert_eq!(result, 1 /* k_EResultOK */, "SendMessageToConnection failed with EResult {result}");

        // Retry briefly rather than assume synchronous delivery.
        let mut received: *mut SteamNetworkingMessage_t = ptr::null_mut();
        let mut got = 0;
        for _ in 0..50 {
            got = SteamAPI_ISteamNetworkingSockets_ReceiveMessagesOnConnection(
                sockets,
                conn_b,
                &mut received,
                1,
            );
            if got > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(got, 1, "expected exactly one message on conn_b");
        assert!(!received.is_null());
        assert_eq!((*received).data(), payload);
        SteamNetworkingMessage_t::release(received);

        SteamAPI_ISteamNetworkingSockets_CloseConnection(sockets, conn_a, 0, ptr::null(), false);
        SteamAPI_ISteamNetworkingSockets_CloseConnection(sockets, conn_b, 0, ptr::null(), false);
        GameNetworkingSockets_Kill();
    }
}

fn cstr(buf: &SteamNetworkingErrMsg) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
