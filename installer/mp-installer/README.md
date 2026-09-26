# mp-installer

Standalone installer: locates the game and the MOMI mods folder, writes
the new mod as a MOMI-conformant package, and places the `mp-relay`
binary.

## Usage

```sh
mp-installer --gml-source <dir> [--relay-binary <path>]
```

## Mod identity

Distinct from the legacy `deulo`/`Multiplayer` mod so MOMI's mod ID and
top-level GML namespace prefix can't collide with it. Keep the constants
in `main.rs` in sync with the GML mod itself once it exists.

## `locator`: game/mods-folder discovery

Reimplemented rather than imported from MOMI (this workspace doesn't
depend on the sibling MOMI repo at all), but deliberately mirrors MOMI's
own `MistriaLocator` algorithm exactly — Steam library candidate paths,
the `Maybe.toml` sentinel, and mods-folder candidate order — since a
different answer here than what MOMI itself would find is the actual
failure mode worth avoiding.

- **`steam_library_locations`**: candidate `steamapps` directories, not
  yet filtered by existence. `home_dir`/`drive_roots` are injected so this
  is testable without touching the real filesystem beyond a tempdir.
- **`real_drive_roots`**: the real OS-provided drive roots to scan on
  Windows (`C:\`, `D:\`, ...) — a single, stable, struct-free Win32 call
  (`GetLogicalDrives`), the same "hand-write the tiny bounded FFI surface"
  discipline as GNS's own bindings, not a `cmake`/`bindgen`-scale
  undertaking. This installer ships for Linux and Windows (a Linux build
  finds the game's Steam library under a native Steam Play/Proton
  install; there's no macOS target). Drive letters aren't a Linux
  concept — its real Steam paths are already covered by the fixed
  home-relative candidates in `steam_library_locations` — so this returns
  nothing there rather than a meaningless Windows-style placeholder.
- **`find_mistria_location`**: the first Steam library candidate whose
  `common/Fields of Mistria/Maybe.toml` exists, or the current working
  directory if *it* has a `Maybe.toml` (matches MOMI's own dev-environment
  fallback).
- **`find_mods_location`**: `<mistria>/mods` or `<mistria>/Mods` (only
  considered if `mistria_location` actually has `Maybe.toml`, i.e. is a
  confirmed real install, not just the cwd fallback), else
  `~/mistria-mods` or `~/Mistria-Mods`. First candidate that exists as a
  directory wins.

### Tests

`find_mistria_location`'s test fabricates a drive root whose
`Steam/steamapps` suffix contains a real install, while every other
candidate the test generates doesn't exist on disk — mirroring the common
case of most real candidates being wrong guesses.

`find_mods_location`'s home-fallback test only creates one of
`mistria-mods`/`Mistria-Mods`: the two alias to the same path on a
case-insensitive filesystem (the default on both macOS and the game's
actual target, Windows), so asserting a specific one would be meaningless.

## `package`: writing the mod package

Writes `manifest.json` and a `gml/` folder, laid out exactly as MOMI's
`FolderMod`/`ModManifest` expect, plus the `mp-relay` binary alongside it.

`ModManifest::mod_id` replicates MOMI's own `FolderMod.Id` derivation
exactly: `<author>.<name>` lowercased, spaces replaced with underscores,
then anything outside `[a-zA-Z0-9_.]` stripped — in that order, so e.g. an
`&` between two space-derived underscores leaves a double underscore
behind. Matching this exactly matters: it's the mod folder name and the
identity MOMI uses to detect an already-installed mod.

`install_mod` is idempotent: re-running against an already-installed copy
overwrites the manifest and replaces `gml/` wholesale (removing stale
files from a previous version) rather than failing because the directory
already exists.

## `legacy`: migration and the conflicting-process warning

`LEGACY_MOD_ID` (`deulo.multiplayer`) is the old mod's MOMI id, through
the same lowercase-dotted derivation as `package::ModManifest::mod_id`.
`remove_legacy_mod` deletes it before installing the new mod — otherwise
MOMI would hit a top-level GML namespace collision merging both mods'
`gml/` together.

`find_recently_active_legacy_relay_dir` is a best-effort signal that an
old relay-based process might still be running and writing to its
shared-file directory. It mirrors the legacy relay's own
`RelayDirectories.ResolveMpDir` (multiple `momi_mp*`/instance folders
under the FieldsOfMistria AppData root); a directory counts as "possibly
active" if its `mp_control.json` was modified recently. This is not a
real running-process check — there's no pid/lock file convention to check
against, only this recency heuristic, the same one the legacy relay
itself already uses to pick which instance is current.

Windows-only for now: on Linux the game runs through Steam Play/Proton,
whose AppData lives inside a per-app `compatdata` prefix keyed by the
game's Steam App ID, which isn't tracked anywhere in this project. This
returns nothing there rather than guess at that path.
