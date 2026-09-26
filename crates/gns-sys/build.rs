// Builds the vendored GameNetworkingSockets static library via CMake and
// links it into this crate. See README.md for why (no cc/cmake crate)
// and the per-platform link libraries.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent() // crates/
        .unwrap()
        .parent() // FOM-Multiplayer/
        .unwrap();
    let gns_src = workspace_root.join("third_party/GameNetworkingSockets");
    if !gns_src.join("CMakeLists.txt").exists() {
        panic!(
            "GameNetworkingSockets submodule not found at {}. Run `git submodule update --init`.",
            gns_src.display()
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let build_dir = out_dir.join("gns-build");

    let openssl_root = brew_prefix("openssl");

    // Configure. Re-running `cmake -S -B` against an already-configured
    // build directory is cheap (it just checks the cache), so we don't try
    // to skip this step.
    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&gns_src)
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DENABLE_ICE=ON")
        .arg("-DUSE_STEAMWEBRTC=OFF")
        .arg("-DBUILD_SHARED_LIB=OFF")
        .arg("-DBUILD_STATIC_LIB=ON")
        .arg("-DBUILD_EXAMPLES=OFF")
        .arg("-DBUILD_TESTS=OFF")
        .arg("-DBUILD_TOOLS=OFF")
        .arg("-DCMAKE_POSITION_INDEPENDENT_CODE=ON");
    if let Some(ref root) = openssl_root {
        configure.arg(format!("-DOPENSSL_ROOT_DIR={}", root.display()));
    }
    run(&mut configure, "cmake configure");

    // Build just the static library target, not the whole tree.
    let mut build = Command::new("cmake");
    build
        .arg("--build")
        .arg(&build_dir)
        .arg("--target")
        .arg("GameNetworkingSockets_s")
        .arg("--config")
        .arg("Release")
        .arg("--parallel");
    run(&mut build, "cmake build");

    // The static lib's exact output directory can vary by generator; search
    // for it rather than assuming a fixed path.
    let lib_dir = find_dir_containing(&build_dir, "libGameNetworkingSockets_s.a")
        .unwrap_or_else(|| panic!("built but couldn't find libGameNetworkingSockets_s.a under {}", build_dir.display()));
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=GameNetworkingSockets_s");

    // GNS's own dependencies: OpenSSL (crypto backend), protobuf (rendezvous
    // message encoding), and the C++ standard library (GNS is C++, we are
    // not, so this must be linked explicitly).
    if let Some(root) = &openssl_root {
        println!("cargo:rustc-link-search=native={}", root.join("lib").display());
    }
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=crypto");
    if let Some(root) = brew_prefix("protobuf") {
        println!("cargo:rustc-link-search=native={}", root.join("lib").display());
    }
    println!("cargo:rustc-link-lib=protobuf");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=framework=Security");
        }
        "linux" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=pthread");
        }
        _ => {}
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", gns_src.join("CMakeLists.txt").display());
    println!("cargo:rerun-if-changed={}", gns_src.join("src/CMakeLists.txt").display());
}

fn run(cmd: &mut Command, what: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {}", what, e));
    if !status.success() {
        panic!("{} failed with {}", what, status);
    }
}

/// Best-effort `$(brew --prefix <formula>)`. Returns None if brew or the
/// formula isn't found; callers fall back to letting CMake search the
/// normal system paths.
fn brew_prefix(formula: &str) -> Option<PathBuf> {
    let output = Command::new("brew").arg("--prefix").arg(formula).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn find_dir_containing(root: &Path, filename: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(filename) {
                return dir.canonicalize().ok();
            }
        }
    }
    None
}
