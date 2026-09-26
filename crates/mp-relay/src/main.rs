//! mp-relay: the native companion process a game instance runs alongside
//! Fields of Mistria. See `README.md` for the shared-file/GNS design.

// Not called from `main` yet; tested on its own (`cargo test -p mp-relay`).
#[allow(dead_code)]
mod file_store;

fn main() {
    eprintln!("mp-relay: not yet implemented (plan milestone 4, Linear TNS-14)");
    std::process::exit(1);
}
