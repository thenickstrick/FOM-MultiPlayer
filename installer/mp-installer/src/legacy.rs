//! Legacy `deulo.multiplayer` mod removal, and a best-effort warning if an
//! old relay-based process might currently be pointed at the same
//! shared-file directory the new mp-relay would use.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The legacy mod's MOMI id (see README.md for the collision it avoids).
pub const LEGACY_MOD_ID: &str = "deulo.multiplayer";

/// Removes the legacy mod's folder from `mods_dir` if present. Returns
/// whether anything was actually removed.
pub fn remove_legacy_mod(mods_dir: &Path) -> io::Result<bool> {
    let legacy_dir = mods_dir.join(LEGACY_MOD_ID);
    if legacy_dir.is_dir() {
        fs::remove_dir_all(&legacy_dir)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Best-effort, Windows-only signal that an old relay might still be
/// running and writing to its shared-file directory (see README.md).
pub fn find_recently_active_legacy_relay_dir(now: SystemTime, recent_within: Duration) -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let fom_root = PathBuf::from(local_app_data).join("FieldsOfMistria");
    find_recently_active_under(&fom_root, now, recent_within)
}

fn find_recently_active_under(fom_root: &Path, now: SystemTime, recent_within: Duration) -> Option<PathBuf> {
    let mut candidates = vec![fom_root.join("momi_mp")];
    if let Ok(entries) = fs::read_dir(fom_root) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("momi_mp");
            if candidate.is_dir() {
                candidates.push(candidate);
            }
        }
    }

    candidates.into_iter().find(|candidate| {
        let modified = fs::metadata(candidate.join("mp_control.json")).and_then(|m| m.modified());
        match modified {
            Ok(modified) => now.duration_since(modified).map(|age| age <= recent_within).unwrap_or(false),
            Err(_) => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-installer-legacy-test-{name}-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn remove_legacy_mod_deletes_the_folder_when_present() {
        let mods_dir = tempdir("remove-present");
        let legacy_dir = mods_dir.join(LEGACY_MOD_ID);
        fs::create_dir_all(legacy_dir.join("gml")).unwrap();
        fs::write(legacy_dir.join("manifest.json"), "{}").unwrap();
        fs::write(legacy_dir.join("gml").join("old.gml"), "// old").unwrap();

        let removed = remove_legacy_mod(&mods_dir).unwrap();

        assert!(removed);
        assert!(!legacy_dir.exists());
    }

    #[test]
    fn remove_legacy_mod_is_a_no_op_when_absent() {
        let mods_dir = tempdir("remove-absent");
        let removed = remove_legacy_mod(&mods_dir).unwrap();
        assert!(!removed);
    }

    #[test]
    fn remove_legacy_mod_does_not_touch_unrelated_mods() {
        let mods_dir = tempdir("remove-unrelated");
        let other_mod = mods_dir.join("someone.else");
        fs::create_dir_all(&other_mod).unwrap();
        fs::write(other_mod.join("manifest.json"), "{}").unwrap();

        remove_legacy_mod(&mods_dir).unwrap();

        assert!(other_mod.exists());
    }

    #[test]
    fn detects_a_recently_written_control_file_as_possibly_active() {
        let fom_root = tempdir("recent-active");
        let momi_mp = fom_root.join("momi_mp");
        fs::create_dir_all(&momi_mp).unwrap();
        fs::write(momi_mp.join("mp_control.json"), "{}").unwrap();

        let now = SystemTime::now();
        let found = find_recently_active_under(&fom_root, now, Duration::from_secs(60));
        assert_eq!(found, Some(momi_mp));
    }

    #[test]
    fn does_not_flag_a_stale_control_file_as_active() {
        let fom_root = tempdir("stale-inactive");
        let momi_mp = fom_root.join("momi_mp");
        fs::create_dir_all(&momi_mp).unwrap();
        fs::write(momi_mp.join("mp_control.json"), "{}").unwrap();

        // Simulate a control file last written well outside the recency
        // window by checking "now" from far enough in the future instead
        // of backdating file mtimes (not portable without extra syscalls).
        let now = SystemTime::now() + Duration::from_secs(3600);
        let found = find_recently_active_under(&fom_root, now, Duration::from_secs(60));
        assert_eq!(found, None);
    }

    #[test]
    fn checks_per_instance_momi_mp_subdirectories_too() {
        // Mirrors the legacy relay's own multi-instance layout:
        // FieldsOfMistria/<instance-id>/momi_mp/mp_control.json.
        let fom_root = tempdir("instance-subdir");
        let instance_momi_mp = fom_root.join("beta").join("momi_mp");
        fs::create_dir_all(&instance_momi_mp).unwrap();
        fs::write(instance_momi_mp.join("mp_control.json"), "{}").unwrap();

        let now = SystemTime::now();
        let found = find_recently_active_under(&fom_root, now, Duration::from_secs(60));
        assert_eq!(found, Some(instance_momi_mp));
    }

    #[test]
    fn returns_none_when_fom_root_does_not_exist() {
        let fom_root = tempdir("missing-root").join("does-not-exist");
        let found = find_recently_active_under(&fom_root, SystemTime::now(), Duration::from_secs(60));
        assert_eq!(found, None);
    }
}
