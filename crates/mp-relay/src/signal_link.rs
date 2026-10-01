//! mp-signal's wire protocol, client side. See `README.md` for why this
//! is duplicated rather than shared via a library crate.

use std::io::{self, Read, Write};
use std::net::TcpStream;

const MAX_FRAME_LEN: u32 = 64 * 1024;
const ROLE_HOST: u8 = 1;
const ROLE_JOIN: u8 = 2;
const STATUS_OK: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Host,
    Join,
}

/// Connects to mp-signal and performs the host/join handshake for `code`,
/// returning the matched, ready-to-relay stream on success.
pub fn rendezvous(signal_addr: &str, role: Role, code: &str) -> io::Result<TcpStream> {
    let mut stream = TcpStream::connect(signal_addr)?;
    let role_byte = match role {
        Role::Host => ROLE_HOST,
        Role::Join => ROLE_JOIN,
    };
    let mut request = vec![role_byte];
    request.extend_from_slice(code.as_bytes());
    write_frame(&mut stream, &request)?;

    let response = read_frame(&mut stream)?;
    if response.first() != Some(&STATUS_OK) {
        return Err(io::Error::other(format!(
            "mp-signal rejected {role:?}/{code}: status {:?}",
            response.first()
        )));
    }
    Ok(stream)
}

pub fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
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

pub fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}
