//! mp-signal: room-code rendezvous relay for GNS's custom-signaling
//! contract. See `README.md` for the protocol and the plaintext rationale.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_PORT: u16 = 7777;
const MAX_FRAME_LEN: u32 = 64 * 1024;
const MAX_CODE_LEN: usize = 32;
const HOST_TIMEOUT: Duration = Duration::from_secs(30);
const REAPER_INTERVAL: Duration = Duration::from_secs(1);

const ROLE_HOST: u8 = 1;
const ROLE_JOIN: u8 = 2;

const STATUS_OK: u8 = 0;
const STATUS_CODE_IN_USE: u8 = 1;
const STATUS_CODE_NOT_FOUND: u8 = 2;
const STATUS_BAD_REQUEST: u8 = 3;

fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind(("0.0.0.0", port)).expect("failed to bind mp-signal listener");
    eprintln!("mp-signal: listening on {}", listener.local_addr().unwrap());

    let registry = Arc::new(RoomRegistry::default());
    spawn_reaper(registry.clone(), HOST_TIMEOUT, REAPER_INTERVAL);

    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("mp-signal: accept error: {e}");
                continue;
            }
        };
        let registry = registry.clone();
        thread::spawn(move || {
            if let Err(e) = handle_connection(stream, &registry) {
                eprintln!("mp-signal: connection error: {e}");
            }
        });
    }
}

/// Waiting hosts, keyed by room code. Each entry is claimed at most once by
/// a matching join, or reaped after `HOST_TIMEOUT` if never claimed.
#[derive(Default)]
struct RoomRegistry {
    waiting: Mutex<HashMap<String, (TcpStream, Instant)>>,
}

enum RegisterError {
    CodeInUse,
}

impl RoomRegistry {
    fn register(&self, code: String, stream: TcpStream) -> Result<(), RegisterError> {
        let mut waiting = self.waiting.lock().unwrap();
        if waiting.contains_key(&code) {
            return Err(RegisterError::CodeInUse);
        }
        waiting.insert(code, (stream, Instant::now()));
        Ok(())
    }

    fn claim(&self, code: &str) -> Option<TcpStream> {
        let mut waiting = self.waiting.lock().unwrap();
        waiting.remove(code).map(|(stream, _)| stream)
    }

    /// Drops (and thereby closes) any registration older than `timeout`.
    fn reap_expired(&self, timeout: Duration) {
        let mut waiting = self.waiting.lock().unwrap();
        waiting.retain(|_, (_, registered_at)| registered_at.elapsed() < timeout);
    }
}

fn spawn_reaper(registry: Arc<RoomRegistry>, timeout: Duration, interval: Duration) {
    thread::spawn(move || loop {
        thread::sleep(interval);
        registry.reap_expired(timeout);
    });
}

fn handle_connection(mut stream: TcpStream, registry: &RoomRegistry) -> io::Result<()> {
    let request = match read_frame(&mut stream) {
        Ok(frame) => frame,
        Err(_) => return Ok(()),
    };

    let (role, code) = match parse_request(&request) {
        Some(parsed) => parsed,
        None => {
            let _ = write_frame(&mut stream, &[STATUS_BAD_REQUEST]);
            return Ok(());
        }
    };

    match role {
        ROLE_HOST => {
            match registry.register(code, stream.try_clone()?) {
                Ok(()) => write_frame(&mut stream, &[STATUS_OK]),
                Err(RegisterError::CodeInUse) => write_frame(&mut stream, &[STATUS_CODE_IN_USE]),
            }
        }
        ROLE_JOIN => match registry.claim(&code) {
            Some(host_stream) => {
                write_frame(&mut stream, &[STATUS_OK])?;
                relay(stream, host_stream)
            }
            None => write_frame(&mut stream, &[STATUS_CODE_NOT_FOUND]),
        },
        _ => unreachable!("parse_request only returns known roles"),
    }
}

fn parse_request(frame: &[u8]) -> Option<(u8, String)> {
    let (&role, code_bytes) = frame.split_first()?;
    if role != ROLE_HOST && role != ROLE_JOIN {
        return None;
    }
    if code_bytes.is_empty() || code_bytes.len() > MAX_CODE_LEN {
        return None;
    }
    let code = std::str::from_utf8(code_bytes).ok()?;
    if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((role, code.to_string()))
}

/// Relays opaque, length-prefixed frames bidirectionally between two peers
/// until either side disconnects or errors, then closes both.
fn relay(a: TcpStream, b: TcpStream) -> io::Result<()> {
    let a_to_b = spawn_pump(a.try_clone()?, b.try_clone()?);
    let b_to_a = spawn_pump(b, a);
    let _ = a_to_b.join();
    let _ = b_to_a.join();
    Ok(())
}

fn spawn_pump(mut from: TcpStream, mut to: TcpStream) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(frame) = read_frame(&mut from) {
            if write_frame(&mut to, &frame).is_err() {
                break;
            }
        }
        let _ = from.shutdown(std::net::Shutdown::Both);
        let _ = to.shutdown(std::net::Shutdown::Both);
    })
}

fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes);
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn start_test_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let registry = Arc::new(RoomRegistry::default());
        spawn_reaper(registry.clone(), Duration::from_millis(100), Duration::from_millis(10));
        thread::spawn(move || {
            for incoming in listener.incoming() {
                let stream = incoming.unwrap();
                let registry = registry.clone();
                thread::spawn(move || {
                    let _ = handle_connection(stream, &registry);
                });
            }
        });
        addr
    }

    fn connect(addr: std::net::SocketAddr) -> TcpStream {
        TcpStream::connect(addr).unwrap()
    }

    #[test]
    fn host_and_join_match_and_relay_opaque_blobs() {
        let addr = start_test_server();

        let mut host = connect(addr);
        write_frame(&mut host, &[[ROLE_HOST].as_slice(), b"ABC123"].concat()).unwrap();
        assert_eq!(read_frame(&mut host).unwrap(), vec![STATUS_OK]);

        let mut join = connect(addr);
        write_frame(&mut join, &[[ROLE_JOIN].as_slice(), b"ABC123"].concat()).unwrap();
        assert_eq!(read_frame(&mut join).unwrap(), vec![STATUS_OK]);

        // Opaque rendezvous blob, host -> join.
        write_frame(&mut host, b"ice-candidate-from-host").unwrap();
        assert_eq!(read_frame(&mut join).unwrap(), b"ice-candidate-from-host");

        // And the reverse direction.
        write_frame(&mut join, b"ice-candidate-from-join").unwrap();
        assert_eq!(read_frame(&mut host).unwrap(), b"ice-candidate-from-join");
    }

    #[test]
    fn duplicate_host_code_is_rejected() {
        let addr = start_test_server();

        let mut host1 = connect(addr);
        write_frame(&mut host1, &[[ROLE_HOST].as_slice(), b"DUP1"].concat()).unwrap();
        assert_eq!(read_frame(&mut host1).unwrap(), vec![STATUS_OK]);

        let mut host2 = connect(addr);
        write_frame(&mut host2, &[[ROLE_HOST].as_slice(), b"DUP1"].concat()).unwrap();
        assert_eq!(read_frame(&mut host2).unwrap(), vec![STATUS_CODE_IN_USE]);
    }

    #[test]
    fn join_with_unknown_code_is_rejected() {
        let addr = start_test_server();

        let mut join = connect(addr);
        write_frame(&mut join, &[[ROLE_JOIN].as_slice(), b"NOSUCH"].concat()).unwrap();
        assert_eq!(read_frame(&mut join).unwrap(), vec![STATUS_CODE_NOT_FOUND]);
    }

    #[test]
    fn malformed_request_is_rejected() {
        let addr = start_test_server();

        let mut conn = connect(addr);
        write_frame(&mut conn, &[9, b'x']).unwrap(); // unknown role byte
        assert_eq!(read_frame(&mut conn).unwrap(), vec![STATUS_BAD_REQUEST]);

        let mut conn2 = connect(addr);
        write_frame(&mut conn2, &[ROLE_HOST]).unwrap(); // empty code
        assert_eq!(read_frame(&mut conn2).unwrap(), vec![STATUS_BAD_REQUEST]);
    }

    #[test]
    fn unclaimed_host_registration_expires() {
        let addr = start_test_server(); // 100ms host timeout, 10ms sweep

        let mut host = connect(addr);
        write_frame(&mut host, &[[ROLE_HOST].as_slice(), b"EXPIRE"].concat()).unwrap();
        assert_eq!(read_frame(&mut host).unwrap(), vec![STATUS_OK]);

        thread::sleep(Duration::from_millis(200));

        // The registration was reaped, so a join with the same code now
        // sees it as unknown.
        let mut join = connect(addr);
        write_frame(&mut join, &[[ROLE_JOIN].as_slice(), b"EXPIRE"].concat()).unwrap();
        assert_eq!(read_frame(&mut join).unwrap(), vec![STATUS_CODE_NOT_FOUND]);
    }

    #[test]
    fn code_can_be_reused_after_being_claimed() {
        let addr = start_test_server();

        let mut host1 = connect(addr);
        write_frame(&mut host1, &[[ROLE_HOST].as_slice(), b"REUSE"].concat()).unwrap();
        assert_eq!(read_frame(&mut host1).unwrap(), vec![STATUS_OK]);

        let mut join1 = connect(addr);
        write_frame(&mut join1, &[[ROLE_JOIN].as_slice(), b"REUSE"].concat()).unwrap();
        assert_eq!(read_frame(&mut join1).unwrap(), vec![STATUS_OK]);

        drop(host1);
        drop(join1);

        let mut host2 = connect(addr);
        write_frame(&mut host2, &[[ROLE_HOST].as_slice(), b"REUSE"].concat()).unwrap();
        assert_eq!(read_frame(&mut host2).unwrap(), vec![STATUS_OK]);
    }
}
