//! Shared-file read/write primitives for the mod/relay handoff directory.
//! See `README.md` for the retry/atomic-write design.

use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

const READ_RETRY_ATTEMPTS: u32 = 6;
const RENAME_RETRY_ATTEMPTS: u32 = 6;
const RETRY_DELAY: Duration = Duration::from_millis(15);

/// Reads a shared file's full contents, retrying on transient errors. A
/// missing file is `Ok(None)`, not an error (see README.md).
pub fn read_shared(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut last_err = None;
    for attempt in 0..READ_RETRY_ATTEMPTS {
        match fs::read(path) {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < READ_RETRY_ATTEMPTS {
                    thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
    Err(last_err.unwrap())
}

/// Writes `data` to `path` atomically via a `.tmp`-then-rename, falling
/// back to a direct write if renames keep failing (see README.md).
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp_path = tmp_path_for(path);
    fs::write(&tmp_path, data)?;

    let mut last_err = None;
    for attempt in 0..RENAME_RETRY_ATTEMPTS {
        match fs::rename(&tmp_path, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < RENAME_RETRY_ATTEMPTS {
                    thread::sleep(RETRY_DELAY);
                }
            }
        }
    }

    match fs::write(path, data) {
        Ok(()) => {
            let _ = fs::remove_file(&tmp_path);
            Ok(())
        }
        Err(_) => Err(last_err.unwrap()),
    }
}

fn tmp_path_for(path: &Path) -> std::path::PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::path::PathBuf::from(tmp)
}

/// Removes stray `.tmp` files left behind by a crash mid-write.
/// Best-effort: individual removal errors are ignored (see README.md).
pub fn cleanup_temp_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
            let _ = fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-relay-file-store-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_shared_returns_none_for_missing_file() {
        let dir = tempdir();
        let result = read_shared(&dir.join("does_not_exist.json")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = tempdir();
        let path = dir.join("out.json");
        write_atomic(&path, b"{\"hello\":\"world\"}").unwrap();
        let read_back = read_shared(&path).unwrap().unwrap();
        assert_eq!(read_back, b"{\"hello\":\"world\"}");
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file_behind_on_success() {
        let dir = tempdir();
        let path = dir.join("out.json");
        write_atomic(&path, b"data").unwrap();
        assert!(!tmp_path_for(&path).exists());
    }

    #[test]
    fn cleanup_temp_files_removes_only_tmp_files() {
        let dir = tempdir();
        fs::write(dir.join("stale.json.tmp"), b"stale").unwrap();
        fs::write(dir.join("real.json"), b"real").unwrap();
        cleanup_temp_files(&dir);
        assert!(!dir.join("stale.json.tmp").exists());
        assert!(dir.join("real.json").exists());
    }

    /// The actual point of tmp+rename: a concurrent reader must only ever
    /// see one of the fully-written values, never a torn/mixed read, even
    /// under sustained concurrent writes.
    #[test]
    fn concurrent_reads_never_observe_a_torn_write() {
        let dir = tempdir();
        let path = Arc::new(dir.join("out.json"));
        let stop = Arc::new(AtomicBool::new(false));

        // Two distinct, easily-distinguished full payloads, each internally
        // consistent, so a torn read would show up as neither.
        let payload_a = vec![b'A'; 4096];
        let payload_b = vec![b'B'; 4096];
        write_atomic(&path, &payload_a).unwrap();

        let writer_path = path.clone();
        let writer_stop = stop.clone();
        let writer = thread::spawn(move || {
            let mut toggle = false;
            while !writer_stop.load(Ordering::Relaxed) {
                let payload = if toggle { &payload_b } else { &payload_a };
                write_atomic(&writer_path, payload).unwrap();
                toggle = !toggle;
            }
        });

        let reader_path = path.clone();
        let reader_stop = stop.clone();
        let reader = thread::spawn(move || {
            let all_a = vec![b'A'; 4096];
            let all_b = vec![b'B'; 4096];
            let mut reads = 0u32;
            while !reader_stop.load(Ordering::Relaxed) || reads < 100 {
                if let Some(bytes) = read_shared(&reader_path).unwrap() {
                    assert!(
                        bytes == all_a || bytes == all_b,
                        "observed a torn write: {} bytes, not matching either full payload",
                        bytes.len()
                    );
                }
                reads += 1;
                if reads > 2000 {
                    break;
                }
            }
        });

        thread::sleep(Duration::from_millis(200));
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        reader.join().unwrap();
    }
}
