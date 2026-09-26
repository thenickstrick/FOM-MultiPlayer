//! Writes the multiplayer mod as a MOMI-conformant package. See
//! `README.md` for the layout and `mod_id` derivation.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct ModManifest {
    pub name: String,
    pub author: String,
    pub version: String,
    pub min_installer_version: String,
    pub manifest_version: String,
}

impl ModManifest {
    /// Replicates MOMI's own `FolderMod.Id` derivation exactly
    /// (README.md) — it's the identity MOMI uses to detect a mod.
    pub fn mod_id(&self) -> String {
        let combined = format!("{}.{}", self.author.to_lowercase(), self.name.to_lowercase()).replace(' ', "_");
        combined.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.').collect()
    }

    pub fn to_json(&self) -> json::JsonValue {
        json::JsonValue::Object(vec![
            ("name".to_string(), json::JsonValue::String(self.name.clone())),
            ("author".to_string(), json::JsonValue::String(self.author.clone())),
            ("version".to_string(), json::JsonValue::String(self.version.clone())),
            (
                "minInstallerVersion".to_string(),
                json::JsonValue::String(self.min_installer_version.clone()),
            ),
            (
                "manifestVersion".to_string(),
                json::JsonValue::String(self.manifest_version.clone()),
            ),
        ])
    }
}

/// Writes the mod package into `<mods_dir>/<mod_id>/`; idempotent
/// (see README.md). Returns the mod's directory.
pub fn install_mod(
    mods_dir: &Path,
    manifest: &ModManifest,
    gml_source_dir: &Path,
    relay_binary_source: Option<&Path>,
) -> io::Result<PathBuf> {
    let mod_dir = mods_dir.join(manifest.mod_id());
    fs::create_dir_all(&mod_dir)?;
    fs::write(mod_dir.join("manifest.json"), json::write(&manifest.to_json()))?;

    let gml_dest = mod_dir.join("gml");
    if gml_dest.exists() {
        fs::remove_dir_all(&gml_dest)?;
    }
    copy_dir_recursive(gml_source_dir, &gml_dest)?;

    if let Some(relay_bin) = relay_binary_source {
        let dest_name = relay_bin
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "relay binary source has no file name"))?;
        fs::copy(relay_bin, mod_dir.join(dest_name))?;
    }

    Ok(mod_dir)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mp-installer-package-test-{name}-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_manifest() -> ModManifest {
        ModManifest {
            name: "Multiplayer".to_string(),
            author: "TNS".to_string(),
            version: "0.1.0".to_string(),
            min_installer_version: "1.0".to_string(),
            manifest_version: "1".to_string(),
        }
    }

    #[test]
    fn mod_id_matches_momis_lowercase_dotted_format() {
        let manifest = sample_manifest();
        assert_eq!(manifest.mod_id(), "tns.multiplayer");
    }

    #[test]
    fn mod_id_strips_spaces_and_invalid_characters() {
        let manifest = ModManifest {
            name: "Multi Player!".to_string(),
            author: "T N S & Co.".to_string(),
            ..sample_manifest()
        };
        // Double underscore is correct here — see README.md.
        assert_eq!(manifest.mod_id(), "t_n_s__co..multi_player");
    }

    #[test]
    fn install_mod_writes_manifest_and_copies_gml_files() {
        let root = tempdir("install-basic");
        let mods_dir = root.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        let gml_source = root.join("gml_source");
        fs::create_dir_all(gml_source.join("nested")).unwrap();
        fs::write(gml_source.join("main.gml"), "// entry point").unwrap();
        fs::write(gml_source.join("nested").join("helper.gml"), "// helper").unwrap();

        let manifest = sample_manifest();
        let mod_dir = install_mod(&mods_dir, &manifest, &gml_source, None).unwrap();

        assert_eq!(mod_dir, mods_dir.join("tns.multiplayer"));
        let written_manifest = fs::read_to_string(mod_dir.join("manifest.json")).unwrap();
        let parsed = json::parse(&written_manifest).unwrap();
        assert_eq!(parsed.get("name").unwrap().as_str(), Some("Multiplayer"));
        assert_eq!(parsed.get("author").unwrap().as_str(), Some("TNS"));

        assert_eq!(fs::read_to_string(mod_dir.join("gml").join("main.gml")).unwrap(), "// entry point");
        assert_eq!(
            fs::read_to_string(mod_dir.join("gml").join("nested").join("helper.gml")).unwrap(),
            "// helper"
        );
    }

    #[test]
    fn install_mod_places_the_relay_binary_alongside_the_package() {
        let root = tempdir("install-with-binary");
        let mods_dir = root.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        let gml_source = root.join("gml_source");
        fs::create_dir_all(&gml_source).unwrap();
        let relay_binary = root.join("mp-relay.exe");
        fs::write(&relay_binary, b"fake binary contents").unwrap();

        let mod_dir = install_mod(&mods_dir, &sample_manifest(), &gml_source, Some(&relay_binary)).unwrap();

        assert_eq!(fs::read(mod_dir.join("mp-relay.exe")).unwrap(), b"fake binary contents");
    }

    #[test]
    fn install_mod_is_idempotent_and_removes_stale_gml_files() {
        let root = tempdir("install-idempotent");
        let mods_dir = root.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        let gml_source = root.join("gml_source");
        fs::create_dir_all(&gml_source).unwrap();
        fs::write(gml_source.join("v1.gml"), "// v1").unwrap();

        let manifest = sample_manifest();
        install_mod(&mods_dir, &manifest, &gml_source, None).unwrap();

        // Simulate a mod update: the source's gml/ contents changed
        // entirely (v1.gml removed, v2.gml added).
        fs::remove_file(gml_source.join("v1.gml")).unwrap();
        fs::write(gml_source.join("v2.gml"), "// v2").unwrap();

        let mod_dir = install_mod(&mods_dir, &manifest, &gml_source, None).unwrap();

        assert!(!mod_dir.join("gml").join("v1.gml").exists(), "stale gml file from the previous install survived");
        assert_eq!(fs::read_to_string(mod_dir.join("gml").join("v2.gml")).unwrap(), "// v2");
    }
}
