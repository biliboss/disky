//! disky desktop GUI — Tauri backend.
//!
//! The backend is a thin client of the `disky` library crate (REQ-DCLEAN-001):
//! engine commands (meter, scan, cleanup, history) are added here as their
//! specs are implemented, each delegating to `disky::*` — never reimplementing
//! file walking, size summing or deletion.

// Keep the engine linked from day one so the dependency edge is real.
use disky as _;

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running disky GUI");
}
