//! mp-relay: the native companion process a game instance runs alongside
//! Fields of Mistria. See `README.md` for the design; usage below.

mod file_store;
mod relay;
mod signal_link;

use gns::Gns;
use relay::SharedFiles;
use signal_link::Role;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, role, signal_addr, code, shared_dir] = args.as_slice() else {
        eprintln!("usage: mp-relay <host|join> <mp-signal-addr> <room-code> <shared-dir>");
        std::process::exit(2);
    };

    let role = match role.as_str() {
        "host" => Role::Host,
        "join" => Role::Join,
        other => {
            eprintln!("unknown role '{other}': expected 'host' or 'join'");
            std::process::exit(2);
        }
    };

    let gns = Gns::init().unwrap_or_else(|e| {
        eprintln!("mp-relay: GNS init failed: {e}");
        std::process::exit(1);
    });

    let files = SharedFiles::new(&PathBuf::from(shared_dir));
    file_store::cleanup_temp_files(&PathBuf::from(shared_dir));

    let deadline = Instant::now() + Duration::from_secs(3600);
    match relay::run(&gns, signal_addr, role, code, &files, deadline, |_| false) {
        Ok(activity) => {
            eprintln!("mp-relay: session ended: {activity:?}");
        }
        Err(e) => {
            eprintln!("mp-relay: session failed: {e}");
            std::process::exit(1);
        }
    }
}
