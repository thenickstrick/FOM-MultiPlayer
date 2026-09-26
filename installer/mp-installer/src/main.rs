//! mp-installer: standalone installer. See `README.md` for usage and design.

mod locator;
mod package;

use std::path::PathBuf;

// Mod identity: see README.md.
const MOD_NAME: &str = "Multiplayer";
const MOD_AUTHOR: &str = "tns";
const MOD_VERSION: &str = "0.1.0";
const MIN_INSTALLER_VERSION: &str = "1.0";
const MANIFEST_VERSION: &str = "1";

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .expect("neither HOME nor USERPROFILE is set")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gml_source = args.iter().position(|a| a == "--gml-source").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let relay_binary = args.iter().position(|a| a == "--relay-binary").and_then(|i| args.get(i + 1)).map(PathBuf::from);

    let Some(gml_source) = gml_source else {
        eprintln!("usage: mp-installer --gml-source <dir> [--relay-binary <path>]");
        std::process::exit(2);
    };

    let home = home_dir();
    let drive_roots = locator::real_drive_roots();
    let cwd = std::env::current_dir().expect("failed to get current directory");

    let mistria_location = locator::find_mistria_location(&home, &drive_roots, &cwd);
    match &mistria_location {
        Some(loc) => eprintln!("mp-installer: found Fields of Mistria at {}", loc.display()),
        None => {
            eprintln!("mp-installer: could not find a Fields of Mistria install; falling back to home-relative mods folders")
        }
    }

    let Some(mods_location) = locator::find_mods_location(mistria_location.as_deref(), &home) else {
        eprintln!("mp-installer: could not find a mods folder (checked the Mistria install and ~/mistria-mods / ~/Mistria-Mods)");
        std::process::exit(1);
    };
    eprintln!("mp-installer: using mods folder {}", mods_location.display());

    let manifest = package::ModManifest {
        name: MOD_NAME.to_string(),
        author: MOD_AUTHOR.to_string(),
        version: MOD_VERSION.to_string(),
        min_installer_version: MIN_INSTALLER_VERSION.to_string(),
        manifest_version: MANIFEST_VERSION.to_string(),
    };

    match package::install_mod(&mods_location, &manifest, &gml_source, relay_binary.as_deref()) {
        Ok(mod_dir) => eprintln!("mp-installer: installed to {}", mod_dir.display()),
        Err(e) => {
            eprintln!("mp-installer: install failed: {e}");
            std::process::exit(1);
        }
    }
}
