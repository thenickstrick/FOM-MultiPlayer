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
    use std::time::Duration;

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

    #[test]
    fn host_and_join_exchange_player_state_and_snapshot_via_mp_signal() {
        let (_server, addr) = spawn_signal_server();
        let gns = Gns::init().expect("GNS init");
        let code = "TESTROOM";

        let host_dir = tempdir("host");
        let join_dir = tempdir("join");
        let host_files = SharedFiles::new(&host_dir);
        let join_files = SharedFiles::new(&join_dir);

        // Seed each side's outgoing files before starting the relay loop,
        // as the GML mod would.
        file_store::write_atomic(&host_files.local_state, br#"{"player_id":"host","x":1,"y":2}"#).unwrap();
        file_store::write_atomic(&join_files.local_state, br#"{"player_id":"join","x":3,"y":4}"#).unwrap();
        file_store::write_atomic(&host_files.snapshot_out, br#"{"tiles":[1,2,3]}"#).unwrap();

        let deadline = Instant::now() + Duration::from_secs(15);

        // Rendezvous both sides before driving either (see README.md).
        let host_stream = signal_link::rendezvous(&addr, Role::Host, code).expect("host rendezvous");
        let join_stream = signal_link::rendezvous(&addr, Role::Join, code).expect("join rendezvous");

        // Only the host sends a snapshot in this test, so the host's stop
        // condition can't wait on `received_snapshot` — nothing ever sends
        // it one.
        let host_handle = thread::spawn(move || {
            drive(&gns, host_stream, false, &host_files, deadline, &mut |a| {
                a.sent_player_state && a.received_player_state && a.sent_snapshot
            })
        });

        // gns is not Clone; drive join on the same instance from this
        // thread by re-deriving a handle. Gns::init() is idempotent
        // (Once-guarded), so this returns the same underlying interface.
        let gns_join = Gns::init().expect("GNS init (join)");
        let join_result = drive(&gns_join, join_stream, true, &join_files, deadline, &mut |a| {
            a.received_player_state && a.received_snapshot
        });

        let host_result = host_handle.join().unwrap();

        let join_activity = join_result.expect("join relay loop failed");
        let host_activity = host_result.expect("host relay loop failed");

        assert!(host_activity.sent_player_state, "host never sent player state");
        assert!(join_activity.received_player_state, "join never received host's player state");
        assert!(join_activity.sent_player_state, "join never sent player state");
        assert!(host_activity.received_player_state, "host never received join's player state");
        assert!(host_activity.sent_snapshot, "host never sent snapshot");
        assert!(join_activity.received_snapshot, "join never received snapshot");

        let received_snapshot = file_store::read_shared(&join_files.snapshot_in).unwrap().unwrap();
        let value = mp_proto::RelayMessage::parse(&received_snapshot).unwrap();
        match value {
            RelayMessage::Snapshot { payload } => {
                assert_eq!(payload.get("tiles").unwrap().as_array().unwrap().len(), 3);
            }
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }
}
