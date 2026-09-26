//! Game/mods-folder discovery. See `README.md` for why this mirrors
//! MOMI's own algorithm instead of importing it.

use std::path::{Path, PathBuf};

const MAYBE_TOML: &str = "Maybe.toml";

/// Candidate Steam library `steamapps` directories, not yet filtered by
/// existence. `home_dir` and `drive_roots` are injected so this is testable
/// without touching the real filesystem beyond a tempdir.
pub fn steam_library_locations(home_dir: &Path, drive_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut locations = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps"),
        PathBuf::from(r"C:\Program Files\Steam\steamapps"),
        home_dir.join("steam/steam/steamapps"),
        home_dir.join("snap/steam/common/.local/share/Steam/steamapps"),
        home_dir.join(".local/share/Steam/steamapps"),
        home_dir.join(".steam/debian-installation/steamapps"),
        home_dir.join("Library/Application Support/CrossOver/Bottles/Steam/drive_c/Program Files (x86)/Steam/steamapps"),
    ];

    const DRIVE_SUFFIXES: &[&str] = &[
        "SteamLibrary/steamapps",
        "Steam/steamapps",
        "Program Files/Steam/steamapps",
        "Program Files (x86)/Steam/steamapps",
        "Program Files/SteamLibrary/steamapps",
        "Program Files (x86)/SteamLibrary/steamapps",
    ];
    for root in drive_roots {
        for suffix in DRIVE_SUFFIXES {
            locations.push(root.join(suffix));
        }
    }

    locations
}

/// The real, OS-provided drive roots to scan on Windows. Empty on Linux
/// (see README.md — drive letters aren't a Linux concept here).
#[cfg(target_os = "windows")]
pub fn real_drive_roots() -> Vec<PathBuf> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| PathBuf::from(format!("{}:\\", (b'A' + bit as u8) as char)))
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn real_drive_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// Finds the Fields of Mistria install directory (see README.md).
pub fn find_mistria_location(home_dir: &Path, drive_roots: &[PathBuf], cwd: &Path) -> Option<PathBuf> {
    let found = steam_library_locations(home_dir, drive_roots)
        .into_iter()
        .map(|steamapps| steamapps.join("common").join("Fields of Mistria"))
        .find(|candidate| candidate.join(MAYBE_TOML).is_file());

    found.or_else(|| {
        if cwd.join(MAYBE_TOML).is_file() {
            Some(cwd.to_path_buf())
        } else {
            None
        }
    })
}

/// Finds the mods folder; first existing candidate wins (see README.md).
pub fn find_mods_location(mistria_location: Option<&Path>, home_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(mistria) = mistria_location {
        if mistria.join(MAYBE_TOML).is_file() {
            candidates.push(mistria.join("mods"));
            candidates.push(mistria.join("Mods"));
        }
    }
    candidates.push(home_dir.join("mistria-mods"));
    candidates.push(home_dir.join("Mistria-Mods"));

    candidates.into_iter().find(|c| c.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-installer-locator-test-{name}-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn steam_library_locations_includes_home_and_drive_candidates() {
        let home = PathBuf::from("/home/player");
        let drives = vec![PathBuf::from("D:\\")];
        let locations = steam_library_locations(&home, &drives);

        assert!(locations.contains(&home.join(".local/share/Steam/steamapps")));
        assert!(locations.contains(&PathBuf::from("D:\\").join("SteamLibrary/steamapps")));
        assert!(locations.contains(&PathBuf::from("D:\\").join("Steam/steamapps")));
    }

    #[test]
    fn find_mistria_location_matches_the_steamapps_candidate_with_maybe_toml() {
        let root = tempdir("mistria-found");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let cwd = root.join("cwd");
        fs::create_dir_all(&cwd).unwrap();

        // A fabricated drive root with a real install (see README.md).
        let drive_root = root.join("D");
        let mistria = drive_root.join("Steam").join("steamapps").join("common").join("Fields of Mistria");
        fs::create_dir_all(&mistria).unwrap();
        fs::write(mistria.join("Maybe.toml"), "").unwrap();

        let found = find_mistria_location(&home, &[drive_root], &cwd).unwrap();
        assert_eq!(found, mistria);
    }

    #[test]
    fn find_mistria_location_falls_back_to_cwd_when_it_has_maybe_toml() {
        let root = tempdir("mistria-cwd-fallback");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let cwd = root.join("cwd");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(cwd.join("Maybe.toml"), "").unwrap();

        let found = find_mistria_location(&home, &[], &cwd).unwrap();
        assert_eq!(found, cwd);
    }

    #[test]
    fn find_mistria_location_returns_none_when_nothing_matches() {
        let root = tempdir("mistria-none");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let cwd = root.join("cwd");
        fs::create_dir_all(&cwd).unwrap();

        assert_eq!(find_mistria_location(&home, &[], &cwd), None);
    }

    fn make_mistria_dir(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("Maybe.toml"), "").unwrap();
    }

    #[test]
    fn find_mods_location_prefers_mistria_mods_folder_over_home_fallback() {
        let root = tempdir("mods-prefer-mistria");
        let mistria = root.join("mistria");
        make_mistria_dir(&mistria);
        let mods = mistria.join("mods");
        fs::create_dir_all(&mods).unwrap();
        let home = root.join("home");
        fs::create_dir_all(home.join("mistria-mods")).unwrap();

        let found = find_mods_location(Some(&mistria), &home).unwrap();
        assert_eq!(found, mods);
    }

    #[test]
    fn find_mods_location_falls_back_to_home_when_mistria_has_no_mods_folder() {
        let root = tempdir("mods-home-fallback");
        let mistria = root.join("mistria");
        make_mistria_dir(&mistria);
        // Deliberately no `mods`/`Mods` under mistria.
        let home = root.join("home");
        // Only create one candidate; see README.md on why.
        fs::create_dir_all(home.join("mistria-mods")).unwrap();

        let found = find_mods_location(Some(&mistria), &home).unwrap();
        assert_eq!(found, home.join("mistria-mods"));
    }

    #[test]
    fn find_mods_location_ignores_mistria_candidates_without_maybe_toml() {
        // A cwd-fallback "mistria location" (no Maybe.toml there) must not
        // contribute mods-folder candidates, matching MOMI's own check.
        let root = tempdir("mods-no-maybe-toml");
        let mistria = root.join("not-really-mistria");
        fs::create_dir_all(mistria.join("mods")).unwrap();
        // No Maybe.toml written.
        let home = root.join("home");
        fs::create_dir_all(home.join("mistria-mods")).unwrap();

        let found = find_mods_location(Some(&mistria), &home).unwrap();
        assert_eq!(found, home.join("mistria-mods"));
    }
}
