//! Wires `file_store`, `gns`, and `signal_link` into the mp-relay poll
//! loop. See `README.md` for the shared-file contract and host/join design.

use crate::file_store;
use crate::signal_link::{self, Role};
use gns::{ConnectionState, Gns, GnsConnection, SignalSender};
use mp_proto::RelayMessage;
use std::io;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RelayActivity {
    pub sent_player_state: bool,
    pub received_player_state: bool,
    pub sent_snapshot: bool,
    pub received_snapshot: bool,
}

#[derive(Clone)]
pub struct SharedFiles {
    pub local_state: PathBuf,
    pub remote_state: PathBuf,
    pub snapshot_out: PathBuf,
    pub snapshot_in: PathBuf,
}

impl SharedFiles {
    pub fn new(dir: &Path) -> Self {
        SharedFiles {
            local_state: dir.join("out.json"),
            remote_state: dir.join("remote.json"),
            snapshot_out: dir.join("world_snapshot_out.json"),
            snapshot_in: dir.join("world_snapshot.json"),
        }
    }
}

/// Runs the relay loop until `deadline` or `stop_when` returns true. See
/// README.md for the host/join initiation design.
pub fn run(
    gns: &Gns,
    signal_addr: &str,
    role: Role,
    code: &str,
    files: &SharedFiles,
    deadline: Instant,
    mut stop_when: impl FnMut(&RelayActivity) -> bool,
) -> io::Result<RelayActivity> {
    let stream = signal_link::rendezvous(signal_addr, role, code)?;
    let initiate = role == Role::Join;
    drive(gns, stream, initiate, files, deadline, &mut stop_when)
}

fn drive(
    gns: &Gns,
    stream: TcpStream,
    initiate: bool,
    files: &SharedFiles,
    deadline: Instant,
    stop_when: &mut dyn FnMut(&RelayActivity) -> bool,
) -> io::Result<RelayActivity> {
    let read_stream = stream.try_clone()?;
    let write_stream = Arc::new(Mutex::new(stream));
    let incoming = spawn_signal_reader(read_stream);

    let mut connection: Option<GnsConnection> = None;
    if initiate {
        connection = Some(gns.connect_p2p_custom_signaling(make_signal_sender(write_stream.clone()), false));
    }

    let mut activity = RelayActivity::default();
    let mut last_sent_snapshot: Option<Vec<u8>> = None;

    while Instant::now() < deadline && !stop_when(&activity) {
        gns.run_callbacks();

        while let Ok(frame) = incoming.try_recv() {
            let write_stream = write_stream.clone();
            if let Some(new_conn) = gns.receive_signal(&frame, move |_hconn| {
                Some(make_signal_sender(write_stream.clone()))
            }) {
                let _ = new_conn.accept();
                connection = Some(new_conn);
            }
        }

        if let Some(conn) = &connection {
            if conn.state() == ConnectionState::Connected {
                pump_outgoing(conn, files, &mut activity, &mut last_sent_snapshot);
                pump_incoming(conn, files, &mut activity)?;
            }
        }

        thread::sleep(Duration::from_millis(10));
    }

    Ok(activity)
}

fn make_signal_sender(write_stream: Arc<Mutex<TcpStream>>) -> SignalSender {
    Box::new(move |blob: &[u8]| {
        let mut stream = write_stream.lock().unwrap();
        signal_link::write_frame(&mut stream, blob).is_ok()
    })
}

fn spawn_signal_reader(mut stream: TcpStream) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(frame) = signal_link::read_frame(&mut stream) {
            if tx.send(frame).is_err() {
                break;
            }
        }
    });
    rx
}

fn pump_outgoing(
    conn: &GnsConnection,
    files: &SharedFiles,
    activity: &mut RelayActivity,
    last_sent_snapshot: &mut Option<Vec<u8>>,
) {
    if let Ok(Some(bytes)) = file_store::read_shared(&files.local_state) {
        if let Some(msg @ RelayMessage::PlayerState { .. }) = RelayMessage::parse(&bytes) {
            if conn.send_unreliable(&msg.to_bytes()).is_ok() {
                activity.sent_player_state = true;
            }
        }
    }

    if let Ok(Some(payload_bytes)) = file_store::read_shared(&files.snapshot_out) {
        if last_sent_snapshot.as_deref() != Some(&payload_bytes[..]) {
            if let Ok(payload) = json::parse(&String::from_utf8_lossy(&payload_bytes)) {
                let msg = RelayMessage::Snapshot { payload };
                if conn.send_reliable(&msg.to_bytes()).is_ok() {
                    activity.sent_snapshot = true;
                    *last_sent_snapshot = Some(payload_bytes);
                }
            }
        }
    }
}

fn pump_incoming(conn: &GnsConnection, files: &SharedFiles, activity: &mut RelayActivity) -> io::Result<()> {
    for raw in conn.poll_messages(32) {
        match RelayMessage::parse(&raw) {
            Some(RelayMessage::PlayerState { .. }) => {
                file_store::write_atomic(&files.remote_state, &raw)?;
                activity.received_player_state = true;
            }
            Some(RelayMessage::Snapshot { .. }) => {
                file_store::write_atomic(&files.snapshot_in, &raw)?;
                activity.received_snapshot = true;
            }
            Some(RelayMessage::SnapshotRequest) | None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};
    use std::sync::OnceLock;
    use std::time::Duration;

    /// Serializes GNS-touching tests against process-global fake loss/lag
    /// settings (see README.md).
    fn gns_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// The real, spawned `mp-signal` process (see README.md).
    struct SignalServer {
        child: Child,
    }

    impl Drop for SignalServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn spawn_signal_server() -> (SignalServer, String) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let candidates = ["debug", "release"];
        let binary = candidates
            .iter()
            .map(|profile| workspace_root.join("target").join(profile).join("mp-signal"))
            .find(|path| path.exists())
            .unwrap_or_else(|| {
                panic!(
                    "mp-signal binary not found under target/{{debug,release}}; run `cargo build -p mp-signal` first"
                )
            });

        let mut child = Command::new(binary)
            .arg("0") // port 0: OS picks an ephemeral port
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn mp-signal");

        let stderr = child.stderr.take().unwrap();
        let mut reader = std::io::BufReader::new(stderr);
        let mut line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut line).expect("failed to read mp-signal startup line");
        // "mp-signal: listening on 127.0.0.1:PORT"
        let addr = line
            .rsplit(' ')
            .next()
            .expect("unexpected mp-signal startup line")
            .trim()
            .to_string();

        // Keep draining stderr in the background so the child never blocks
        // on a full pipe buffer.
        thread::spawn(move || {
            let mut sink = String::new();
            while std::io::BufRead::read_line(&mut reader, &mut sink).unwrap_or(0) > 0 {
                sink.clear();
            }
        });

        (SignalServer { child }, addr)
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-relay-integration-{name}-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Rendezvous-es both sides, then drives both relay loops concurrently
    /// to their stop conditions or `deadline` (see README.md).
    fn connect_and_drive(
        addr: &str,
        code: &str,
        host_files: SharedFiles,
        join_files: SharedFiles,
        deadline: Instant,
        mut host_stop: impl FnMut(&RelayActivity) -> bool + Send + 'static,
        mut join_stop: impl FnMut(&RelayActivity) -> bool,
    ) -> (io::Result<RelayActivity>, io::Result<RelayActivity>) {
        let gns_host = Gns::init().expect("GNS init (host)");
        let gns_join = Gns::init().expect("GNS init (join)");

        let host_stream = signal_link::rendezvous(addr, Role::Host, code).expect("host rendezvous");
        let join_stream = signal_link::rendezvous(addr, Role::Join, code).expect("join rendezvous");

        let host_handle =
            thread::spawn(move || drive(&gns_host, host_stream, false, &host_files, deadline, &mut host_stop));
        let join_result = drive(&gns_join, join_stream, true, &join_files, deadline, &mut join_stop);
        let host_result = host_handle.join().unwrap();
        (host_result, join_result)
    }

    /// Like `connect_and_drive` but hands back the live connections
    /// instead of driving them (see README.md).
    fn connect_pair_raw<'g>(gns_host: &'g Gns, gns_join: &'g Gns, addr: &str, code: &str) -> (GnsConnection<'g>, GnsConnection<'g>) {
        let host_stream = signal_link::rendezvous(addr, Role::Host, code).expect("host rendezvous");
        let join_stream = signal_link::rendezvous(addr, Role::Join, code).expect("join rendezvous");

        let host_incoming = spawn_signal_reader(host_stream.try_clone().unwrap());
        let host_write = Arc::new(Mutex::new(host_stream));
        let join_incoming = spawn_signal_reader(join_stream.try_clone().unwrap());
        let join_write = Arc::new(Mutex::new(join_stream));

        let join_conn = gns_join.connect_p2p_custom_signaling(make_signal_sender(join_write), false);
        let mut host_conn: Option<GnsConnection> = None;

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            gns_host.run_callbacks();
            gns_join.run_callbacks();

            while let Ok(frame) = join_incoming.try_recv() {
                let _ = gns_join.receive_signal(&frame, |_hconn| None);
            }
            while let Ok(frame) = host_incoming.try_recv() {
                let host_write = host_write.clone();
                if let Some(conn) = gns_host.receive_signal(&frame, move |_hconn| Some(make_signal_sender(host_write.clone()))) {
                    let _ = conn.accept();
                    host_conn = Some(conn);
                }
            }

            let host_connected = host_conn.as_ref().map(|c| c.state()) == Some(ConnectionState::Connected);
            if join_conn.state() == ConnectionState::Connected && host_connected {
                break;
            }
            if Instant::now() > deadline {
                panic!("connect_pair_raw: did not reach Connected in time");
            }
            thread::sleep(Duration::from_millis(10));
        }
        (host_conn.unwrap(), join_conn)
    }

    fn seed_player_states(host_files: &SharedFiles, join_files: &SharedFiles) {
        file_store::write_atomic(&host_files.local_state, br#"{"player_id":"host","x":1,"y":2}"#).unwrap();
        file_store::write_atomic(&join_files.local_state, br#"{"player_id":"join","x":3,"y":4}"#).unwrap();
    }

    #[test]
    fn host_and_join_exchange_player_state_and_snapshot_via_mp_signal() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();

        let host_files = SharedFiles::new(&tempdir("host"));
        let join_files = SharedFiles::new(&tempdir("join"));
        seed_player_states(&host_files, &join_files);
        file_store::write_atomic(&host_files.snapshot_out, br#"{"tiles":[1,2,3]}"#).unwrap();

        let deadline = Instant::now() + Duration::from_secs(15);
        // Only the host sends a snapshot in this test, so the host's stop
        // condition can't wait on `received_snapshot` — nothing ever sends
        // it one.
        let (host_result, join_result) = connect_and_drive(
            &addr,
            "TESTROOM",
            host_files.clone(),
            join_files.clone(),
            deadline,
            |a| a.sent_player_state && a.received_player_state && a.sent_snapshot,
            |a| a.received_player_state && a.received_snapshot,
        );

        let join_activity = join_result.expect("join relay loop failed");
        let host_activity = host_result.expect("host relay loop failed");

        assert!(host_activity.sent_player_state, "host never sent player state");
        assert!(join_activity.received_player_state, "join never received host's player state");
        assert!(join_activity.sent_player_state, "join never sent player state");
        assert!(host_activity.received_player_state, "host never received join's player state");
        assert!(host_activity.sent_snapshot, "host never sent snapshot");
        assert!(join_activity.received_snapshot, "join never received snapshot");

        let received_snapshot = file_store::read_shared(&join_files.snapshot_in).unwrap().unwrap();
        match RelayMessage::parse(&received_snapshot).unwrap() {
            RelayMessage::Snapshot { payload } => {
                assert_eq!(payload.get("tiles").unwrap().as_array().unwrap().len(), 3);
            }
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }

    #[test]
    fn full_snapshot_transfer_in_both_directions() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();

        let host_files = SharedFiles::new(&tempdir("host-bidi-snap"));
        let join_files = SharedFiles::new(&tempdir("join-bidi-snap"));
        file_store::write_atomic(&host_files.snapshot_out, br#"{"from":"host"}"#).unwrap();
        file_store::write_atomic(&join_files.snapshot_out, br#"{"from":"join"}"#).unwrap();

        let deadline = Instant::now() + Duration::from_secs(15);
        let (host_result, join_result) = connect_and_drive(
            &addr,
            "BIDISNAP",
            host_files.clone(),
            join_files.clone(),
            deadline,
            |a| a.sent_snapshot && a.received_snapshot,
            |a| a.sent_snapshot && a.received_snapshot,
        );

        assert!(host_result.expect("host relay loop failed").received_snapshot);
        assert!(join_result.expect("join relay loop failed").received_snapshot);

        let host_got = file_store::read_shared(&host_files.snapshot_in).unwrap().unwrap();
        let join_got = file_store::read_shared(&join_files.snapshot_in).unwrap().unwrap();
        match RelayMessage::parse(&host_got).unwrap() {
            RelayMessage::Snapshot { payload } => assert_eq!(payload.get("from").unwrap().as_str(), Some("join")),
            other => panic!("expected Snapshot, got {other:?}"),
        }
        match RelayMessage::parse(&join_got).unwrap() {
            RelayMessage::Snapshot { payload } => assert_eq!(payload.get("from").unwrap().as_str(), Some("host")),
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }

    #[test]
    fn reconnect_cycle_after_a_completed_session_succeeds_again() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();
        let code = "RECONNECT";

        for attempt in 1..=2 {
            let host_files = SharedFiles::new(&tempdir(&format!("host-reconnect-{attempt}")));
            let join_files = SharedFiles::new(&tempdir(&format!("join-reconnect-{attempt}")));
            seed_player_states(&host_files, &join_files);

            let deadline = Instant::now() + Duration::from_secs(15);
            let (host_result, join_result) = connect_and_drive(
                &addr,
                code,
                host_files.clone(),
                join_files.clone(),
                deadline,
                |a| a.sent_player_state && a.received_player_state,
                |a| a.sent_player_state && a.received_player_state,
            );
            host_result.unwrap_or_else(|e| panic!("attempt {attempt}: host relay loop failed: {e}"));
            join_result.unwrap_or_else(|e| panic!("attempt {attempt}: join relay loop failed: {e}"));
            // Each iteration is a genuinely fresh session (see README.md).
        }
    }

    #[test]
    fn host_shutdown_mid_session_is_observed_by_peer() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();
        let gns_host = Gns::init().expect("GNS init (host)");
        let gns_join = Gns::init().expect("GNS init (join)");

        let (host_conn, join_conn) = connect_pair_raw(&gns_host, &gns_join, &addr, "SHUTDOWN1");
        assert_eq!(join_conn.state(), ConnectionState::Connected);

        drop(host_conn); // simulates the host process going away mid-session

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            gns_join.run_callbacks();
            if join_conn.state() != ConnectionState::Connected {
                return; // peer noticed — test passes
            }
            if Instant::now() > deadline {
                panic!("join side never noticed host shutdown; still Connected after the deadline");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn concurrent_independent_sessions_do_not_interfere() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();

        let run_session = |code: &'static str, tag: &'static str| {
            let addr = addr.clone();
            thread::spawn(move || {
                let host_files = SharedFiles::new(&tempdir(&format!("host-concurrent-{tag}")));
                let join_files = SharedFiles::new(&tempdir(&format!("join-concurrent-{tag}")));
                file_store::write_atomic(
                    &host_files.local_state,
                    format!(r#"{{"player_id":"host-{tag}","tag":"{tag}"}}"#).as_bytes(),
                )
                .unwrap();

                let deadline = Instant::now() + Duration::from_secs(15);
                let (host_result, join_result) = connect_and_drive(
                    &addr,
                    code,
                    host_files.clone(),
                    join_files.clone(),
                    deadline,
                    |a| a.sent_player_state,
                    |a| a.received_player_state,
                );
                host_result.unwrap();
                join_result.unwrap();

                let received = file_store::read_shared(&join_files.remote_state).unwrap().unwrap();
                match RelayMessage::parse(&received).unwrap() {
                    RelayMessage::PlayerState { player_id, .. } => assert_eq!(player_id, format!("host-{tag}")),
                    other => panic!("expected PlayerState, got {other:?}"),
                }
            })
        };

        let session_a = run_session("CONCURA", "a");
        let session_b = run_session("CONCURB", "b");
        session_a.join().unwrap();
        session_b.join().unwrap();
    }

    #[test]
    fn connection_survives_simulated_lag_and_packet_loss() {
        let _lock = gns_test_lock().lock().unwrap();
        let (_server, addr) = spawn_signal_server();
        let gns_for_conditions = Gns::init().expect("GNS init (conditions)");

        struct ResetConditionsOnDrop<'g>(&'g Gns);
        impl Drop for ResetConditionsOnDrop<'_> {
            fn drop(&mut self) {
                self.0.clear_simulated_network_conditions();
            }
        }
        gns_for_conditions.simulate_network_conditions(5.0, 50);
        let _reset_guard = ResetConditionsOnDrop(&gns_for_conditions);

        let host_files = SharedFiles::new(&tempdir("host-lag-loss"));
        let join_files = SharedFiles::new(&tempdir("join-lag-loss"));
        seed_player_states(&host_files, &join_files);

        // Generous deadline: 5% loss + 50ms lag slows the handshake and
        // retransmits noticeably versus loopback-with-no-degradation.
        let deadline = Instant::now() + Duration::from_secs(30);
        let (host_result, join_result) = connect_and_drive(
            &addr,
            "LAGLOSS",
            host_files.clone(),
            join_files.clone(),
            deadline,
            |a| a.sent_player_state && a.received_player_state,
            |a| a.sent_player_state && a.received_player_state,
        );
        host_result.expect("host relay loop failed under simulated lag/loss");
        join_result.expect("join relay loop failed under simulated lag/loss");
    }
}
