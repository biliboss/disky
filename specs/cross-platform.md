---
id: XPLAT-DOC
title: Windows and Linux support
owner: Selvakumar Esra
priority: high
version: 1
---

<!-- id: XPLAT-DOC -->
# Windows and Linux support

disky was built for macOS. This document makes the engine, the CLI, and the desktop GUI run on Linux and Windows with the same commands, JSON envelopes, and safety rules. The CLI stays canonical (AGENTS.md), and every rule in `disk-cleanup.md` and `disk-screens.md` keeps applying on each OS. Only the places those specs call macOS-specific (folder locations, the Trash, Finder, `df`) change.

In scope: path handling, default folders, disk space, locking, cleanup locations, Trash and restore, open and reveal, agent detection, GUI bundles, and CI and release builds.

Out of scope: a per-OS visual theme (the GUI keeps the Organic design everywhere), app-leftover detection outside macOS (no installed-app list exists, so it reports nothing, per REQ-DCLEAN-033), and code signing or notarization for the Windows and Linux bundles.

Supported targets: macOS (aarch64, x86_64), Linux (x86_64, aarch64, glibc), and Windows 10/11 (x86_64, MSVC).

<!-- id: REQ-XPLAT-001 -->
## Paths MUST use `/` separators in every snapshot, envelope and history entry

On Windows, the scan records `C:\Users\me\x` as `C:/Users/me/x`. Every path the engine prints, stores, or compares uses `/`, so the folder-prefix logic (map, cleanup scope, nesting, exclusions) is the same code on every OS. A path the user types may use either separator.

implementations:
  - src/platform.rs:to_slash
  - src/scan.rs:run_observed

verifies:
  - src/platform.rs:windows_paths_become_forward_slashes
  - src/platform.rs:unix_paths_are_unchanged
  - src/map.rs:drive_root_children_are_listed

## Acceptance
<!-- id: REQ-XPLAT-001.A1 -->
- A Windows path `C:\Users\me\a b\x.txt` is recorded as `C:/Users/me/a b/x.txt`, and its `name` is `x.txt`.
<!-- id: REQ-XPLAT-001.A2 -->
- `disky map C:\Users\me` and `disky map C:/Users/me` return the same envelope.
<!-- id: REQ-XPLAT-001.A3 -->
- A drive root `C:/` lists its immediate children (`C:/Users`, `C:/Windows`, …), the same way `/` does on Unix.
<!-- id: REQ-XPLAT-001.A4 -->
- On macOS and Linux, paths are recorded byte-for-byte as before (no snapshot change).

<!-- id: REQ-XPLAT-002 -->
## The default scan root, home and data folder MUST follow each OS's convention

implementations:
  - src/platform.rs:home_dir
  - src/platform.rs:default_root
  - src/snapshots.rs:snapshot_dir

verifies:
  - src/platform.rs:home_prefers_the_environment
  - tests/agentic.rs:prefs_prints_every_key_with_its_default
  - tests/agentic.rs:in_home

## Acceptance
<!-- id: REQ-XPLAT-002.A1 -->
- With no path, `disky scan` scans `/` on macOS and Linux, and the system drive root (`%SystemDrive%/`, else `C:/`) on Windows.
<!-- id: REQ-XPLAT-002.A2 -->
- Home is `$HOME` on macOS and Linux and `%USERPROFILE%` on Windows, read from the environment first, so tests can point it at a temp folder.
<!-- id: REQ-XPLAT-002.A3 -->
- Snapshots, `history.json` and `prefs.json` live in `~/Library/Application Support/disky` (macOS), `$XDG_DATA_HOME/disky` or else `~/.local/share/disky` (Linux), and `%LOCALAPPDATA%\disky` (Windows). On Windows the environment variable wins.

<!-- id: REQ-XPLAT-003 -->
## Disk capacity and free space MUST come from the OS on every platform

The meter and the `disk` object report what the OS reports: `statvfs` on Unix and `GetDiskFreeSpaceExW` on Windows. They are never a sum of file sizes (REQ-DCLEAN-002.A1).

implementations:
  - src/disk.rs:disk_usage

verifies:
  - src/disk.rs:disk_usage_reports_physical_volume_space
  - src/disk.rs:disk_usage_fails_for_missing_path

## Acceptance
<!-- id: REQ-XPLAT-003.A1 -->
- On Windows, `capacity_bytes` is the total bytes and `free_bytes` the bytes free to the caller, for the volume holding the path. `volume` is the drive (`C:`).
<!-- id: REQ-XPLAT-003.A2 -->
- On Linux, `volume` is the mount point's name, or `/` for the root file system.
<!-- id: REQ-XPLAT-003.A3 -->
- A missing path fails with an error on every OS (no zeroes).

<!-- id: REQ-XPLAT-004 -->
## History and preferences writes MUST hold an exclusive lock on every OS

The `history.lock` and `prefs.lock` rules (REQ-DCLEAN-034) use the OS's file lock (`flock` on Unix, `LockFileEx` on Windows). The lock is released when the holder exits.

The temp file is swapped in with one rename. Windows refuses to replace a file that another process (a reader, an indexer or antivirus) holds open for a moment, so there a denied replace is retried for up to about half a second before it fails.

implementations:
  - src/history.rs:Lock
  - src/platform.rs:replace_file

verifies:
  - src/history.rs:concurrent_appends_lose_nothing
  - src/prefs.rs:concurrent_writers_never_truncate_or_lose_the_file

## Acceptance
<!-- id: REQ-XPLAT-004.A1 -->
- Two writers appending at the same time on any OS keep both entries.
<!-- id: REQ-XPLAT-004.A2 -->
- Eight threads setting and reading preferences at once on Windows all succeed, and the store stays valid.

<!-- id: REQ-XPLAT-005 -->
## Cleanup categories MUST target each OS's own folders

The category table (REQ-DCLEAN-033) keeps the same names, labels and icons everywhere. Each location category matches the folders below on each OS. A location that doesn't exist yields nothing.

| Category | macOS | Linux | Windows |
|---|---|---|---|
| system_caches | `/Library/Caches` (safe) | `/var/cache` (review; skips `debconf`, `ldconfig`) | `C:/Windows/SoftwareDistribution/Download` (review) |
| app_caches | `~/Library/Caches` (safe) | `~/.cache` (safe) | `~/AppData/Local/Microsoft/Windows/INetCache` (safe) |
| temp | `/private/tmp` (safe), `/private/var/folders/*/*/T` (review) | `/tmp` (review), `/var/tmp` (review) | `~/AppData/Local/Temp` (review), `C:/Windows/Temp` (review) |
| logs | `~/Library/Logs`, `/Library/Logs` | `/var/log` (review) | `C:/Windows/Logs` |
| trash | `~/.Trash` | `~/.local/share/Trash/files` | `C:/$Recycle.Bin/*` |
| app_leftovers | as before | none | none |
| old_downloads | `~/Downloads` | `~/Downloads` | `~/Downloads` |

The dev categories (`node_modules`, `target`, …) match by folder name on every OS.

implementations:
  - src/cleanup.rs:CATEGORIES
  - src/cleanup.rs:OsLocations
  - src/cleanup.rs:OS_LOCATIONS

verifies:
  - src/cleanup.rs:every_os_table_names_the_same_categories
  - src/cleanup.rs:linux_temp_skips_live_sockets

## Acceptance
<!-- id: REQ-XPLAT-005.A1 -->
- `disky schema` and the GUI list the same category names on all three OSes.
<!-- id: REQ-XPLAT-005.A2 -->
- On Linux, `/tmp/.X11-unix`, `/tmp/.ICE-unix`, `/tmp/systemd-private-*`, `ssh-*` and `tmux-*` are never offered.
<!-- id: REQ-XPLAT-005.A3 -->
- Outside macOS, `app_leftovers` yields no hits.

<!-- id: REQ-XPLAT-006 -->
## Move to Trash MUST use the OS Trash, and restore MUST undo it

`--reversible` and the GUI's default clean move each path to the Trash the OS's own file manager shows, so the user can also restore it there.

- macOS: `~/.Trash/<name>-<stamp>`, as before.
- Linux: the freedesktop.org Trash, `~/.local/share/Trash` (`$XDG_DATA_HOME/Trash`): `files/<name>` plus `info/<name>.trashinfo` with the original path and deletion time.
- Windows: the Recycle Bin of the path's drive, `<drive>/$Recycle.Bin/<user SID>/$R<id><ext>` plus the `$I<id><ext>` record (format 2) Explorer reads.

Restore (REQ-DCLEAN-034) moves the item back and removes the `.trashinfo` or `$I` record. Emptying the `trash` category permanently also removes that record.

implementations:
  - src/trash.rs:Bin
  - src/trash.rs:user_root
  - src/trash.rs:forget_record
  - src/trash.rs:trashinfo
  - src/trash.rs:recycle_record
  - src/trash.rs:token_user_sid
  - src/cleanup.rs:apply_one
  - src/history.rs:restore

verifies:
  - src/trash.rs:flat_put_keeps_the_macos_layout
  - src/trash.rs:another_drive_uses_its_own_bin
  - src/trash.rs:freedesktop_put_writes_files_and_info
  - src/trash.rs:freedesktop_names_never_collide
  - src/trash.rs:recycle_put_writes_r_and_i_files
  - src/trash.rs:recycle_record_is_format_2
  - src/trash.rs:forget_record_removes_the_sidecar
  - src/history.rs:restore_removes_the_trash_record
  - src/trash.rs:the_user_sid_comes_from_the_process_token

## Acceptance
<!-- id: REQ-XPLAT-006.A1 -->
- On Linux, after a reversible clean of `/home/me/p/node_modules`, `files/node_modules` exists and `info/node_modules.trashinfo` holds `Path=/home/me/p/node_modules` and a `DeletionDate=YYYY-MM-DDThh:mm:ss`.
<!-- id: REQ-XPLAT-006.A2 -->
- A second trashed item with the same name becomes `node_modules.2` (then `.3`, …), and nothing is overwritten.
<!-- id: REQ-XPLAT-006.A3 -->
- On Windows, the `$I` record is little-endian: version 2, the size, the deletion time as a FILETIME, the path length in UTF-16 units including the NUL, then the path with `\` separators.
<!-- id: REQ-XPLAT-006.A4 -->
- `disky restore <id>` puts the item back and leaves no `.trashinfo` or `$I` record behind. Conflicts and missing items behave as REQ-DCLEAN-034 says.
<!-- id: REQ-XPLAT-006.A5 -->
- A path on another volume than the Trash fails as "move to Trash failed" and stays where it is. Nothing is copied and then deleted.
<!-- id: REQ-XPLAT-006.A6 -->
- On Windows, the user's SID comes from the process's own token, so finding the Recycle Bin needs no helper program.

<!-- id: REQ-XPLAT-007 -->
## Physical size MUST be reported on every OS

On Windows, a file's physical size is its allocated size when the OS reports it, else its length. Hard links to one file count once on every OS (REQ-DSCR-005.A4): the identity is `(dev, ino)` on Unix and the volume serial number plus file index on Windows. A file whose identity cannot be read counts as its own file.

implementations:
  - src/scan.rs:run_observed
  - src/contract.rs:physical_bytes_on_disk
  - src/dupes.rs:inode

verifies:
  - src/scan.rs:physical_size_is_recorded_for_files
  - src/dupes.rs:hard_links_count_once

## Acceptance
<!-- id: REQ-XPLAT-007.A1 -->
- Every regular file in a Windows snapshot has a non-null `physical_size`.
<!-- id: REQ-XPLAT-007.A2 -->
- Two hard links to one file are never a duplicate group, on Windows as on Unix.

<!-- id: REQ-XPLAT-008 -->
## Open and reveal MUST use each OS's file manager

`disky render` opening the report, the TUI's open key, and the GUI's Reveal in Finder use `open` / `open -R` on macOS, `explorer <path>` / `explorer /select,<path>` on Windows, and `xdg-open` on Linux. Reveal on Linux asks the file manager to select the item over D-Bus (`org.freedesktop.FileManager1.ShowItems`), and opens the parent folder when that fails. The GUI labels the action "Reveal in Finder" on macOS, "Show in Explorer" on Windows, and "Show in folder" on Linux.

implementations:
  - src/platform.rs:open_path
  - src/platform.rs:reveal_path
  - src/platform.rs:open_command
  - src/platform.rs:reveal_commands
  - gui/src-tauri/src/main.rs:reveal_in_finder
  - gui/src/lib/os.ts:revealLabel

verifies:
  - src/platform.rs:open_and_reveal_commands_per_os
  - gui/src/lib/os.test.ts:eachOsHasItsOwnWords

## Acceptance
<!-- id: REQ-XPLAT-008.A1 -->
- On Windows, reveal runs `explorer /select,C:\Users\me\x.iso` with `\` separators.
<!-- id: REQ-XPLAT-008.A2 -->
- On Linux, when the D-Bus call fails, reveal opens the parent folder with `xdg-open`. When both fail, the GUI shows its existing error (REQ-DSCR-012).

<!-- id: REQ-XPLAT-009 -->
## Agent detection MUST find Windows launchers

On Windows, `disky agents` looks for `<bin>.exe`, `<bin>.cmd` and `<bin>.bat` in `PATH`, then `%APPDATA%/npm` and `~/.local/bin`. On Unix the rules of REQ-DSCR-007 apply unchanged.

implementations:
  - src/agents.rs:search_dirs
  - src/agents.rs:candidates

verifies:
  - src/agents.rs:windows_candidates_include_cmd_launchers

## Acceptance
<!-- id: REQ-XPLAT-009.A1 -->
- With `claude.cmd` in `%APPDATA%\npm`, Claude Code is reported `found` with that path.

<!-- id: REQ-XPLAT-010 -->
## The GUI MUST build and bundle on Windows and Linux

The Tauri app builds an NSIS installer on Windows and a `.deb` and AppImage on Linux. The macOS window chrome (overlay title bar, traffic-light position) applies only on macOS, and the other OSes keep their native title bar.

implementations:
  - gui/src-tauri/tauri.windows.conf.json
  - gui/src-tauri/tauri.linux.conf.json
  - gui/vite.config.ts
  - gui/scripts/lint-tokens.mjs
  - gui/scripts/lint-controls.mjs
  - gui/scripts/lint-boundary.mjs

## Acceptance
<!-- id: REQ-XPLAT-010.A1 -->
- `npm run tauri build` produces the NSIS installer on Windows and the `.deb` and AppImage on Linux.
<!-- id: REQ-XPLAT-010.A2 -->
- The GUI's screens, flows and tests are the same on every OS. Only the Reveal label (REQ-XPLAT-008) differs.
<!-- id: REQ-XPLAT-010.A3 -->
- The GUI's unit tests and lints pass on whichever OS runs them. The tests pin the macOS user agent, as the Playwright config does, and the lint scripts resolve their own folder with `fileURLToPath`, so a Windows path never reads as `D:\D:\…`.

<!-- id: REQ-XPLAT-011 -->
## CI MUST test, and Release MUST ship, every supported OS

Windows jobs run on `windows-2022` (Visual Studio 2022). The bundled DuckDB (`libduckdb-sys` 1.10502) does not compile with Visual Studio 2026's STL, which drops `stdext::checked_array_iterator`. A newer DuckDB builds there, but it needs Rust 1.88 and may change the snapshot format, so moving to it is a separate decision [needs review].

implementations:
  - .github/workflows/ci.yml
  - .github/workflows/release.yml

## Acceptance
<!-- id: REQ-XPLAT-011.A1 -->
- CI runs fmt, clippy and the engine's tests on macOS, Ubuntu and Windows on every push and PR. An unreachable build cache never fails a job: CI builds without `sccache` when its server cannot start.
<!-- id: REQ-XPLAT-011.A2 -->
- CI builds the GUI's Tauri crate on Ubuntu and Windows.
<!-- id: REQ-XPLAT-011.A3 -->
- A release tag uploads `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` archives next to the two macOS tarballs.
