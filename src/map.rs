//! Space map (REQ-DSCR-003): a folder's immediate children by physical size.
//!
//! `disky map [<path>]` and the GUI's Space map read the same [`map`] call:
//! one grouped DuckDB query over the snapshot rows beneath the folder — no
//! per-child loop — so a 1 M-row snapshot answers in well under a second.
//!
//! @implements REQ-DSCR-003

use anyhow::Result;
use duckdb::types::Value;
use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::exit::{DiskyError, ExitCode};

/// One immediate child of the mapped folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapChild {
    /// Basename.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// Physical bytes of everything beneath it (itself, for a file).
    pub bytes: u64,
    /// True when anything sits beneath it in the snapshot.
    pub has_children: bool,
}

/// A mapped folder: what `disky map` prints and the Space map renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapView {
    /// The folder mapped (absolute, no trailing `/` except for `/`).
    pub path: String,
    /// Its basename, or `/` for the root of the filesystem.
    pub name: String,
    /// The snapshot's scan root, so a client knows where "Up" stops.
    pub root: String,
    /// Physical bytes under the folder: the sum of `records[].bytes`.
    pub total_bytes: u64,
    /// Capacity of the volume holding the scan root, for share of disk.
    pub capacity_bytes: u64,
    /// Immediate children, largest first (ties by name).
    pub records: Vec<MapChild>,
    /// Children folded away by [`MapView::fold`]: how many, and their bytes.
    /// `None` when nothing was folded.
    pub other: Option<MapOther>,
}

/// The children a folded map leaves out, summed by the engine so a client
/// can draw one "Other" block without adding sizes itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapOther {
    pub count: u64,
    pub bytes: u64,
}

/// Most blocks the Space map draws (REQ-DSCR-003.A5).
pub const MAX_TILES: usize = 20;
/// A child below this share of the folder is folded into "Other": a smaller
/// block is too thin to label or click (REQ-DSCR-003.A5).
pub const MIN_TILE_SHARE: f64 = 0.005;

impl MapView {
    /// Keep at most `max_tiles - 1` children (the last slot is "Other") whose
    /// bytes are at least `min_share` of the folder, and fold the rest into
    /// [`MapView::other`]. `total_bytes` is unchanged. Folding nothing leaves
    /// the view as it is (REQ-DSCR-003.A5).
    pub fn fold(mut self, max_tiles: usize, min_share: f64) -> Self {
        let floor = (self.total_bytes as f64 * min_share).ceil() as u64;
        let keep_max = max_tiles.saturating_sub(1).max(1);
        let keep = self
            .records
            .iter()
            .take_while(|c| c.bytes >= floor)
            .count()
            .min(keep_max);
        // Folding a single child would only rename it: keep it as itself.
        if self.records.len() - keep <= 1 && self.records.len() <= max_tiles {
            return self;
        }
        let rest = self.records.split_off(keep);
        self.other = Some(MapOther {
            count: rest.len() as u64,
            bytes: rest.iter().map(|c| c.bytes).sum(),
        });
        self
    }
}

/// `path` without a trailing `/`; the filesystem root stays `/`.
pub fn normalize(path: &str) -> String {
    let path = crate::platform::to_slash_str(path);
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        "/".to_string()
    } else {
        t.to_string()
    }
}

fn basename(path: &str) -> String {
    match path.rsplit('/').next() {
        Some(b) if !b.is_empty() => b.to_string(),
        _ => "/".to_string(),
    }
}

/// True when `path` is `folder` or lies beneath it (whole components).
fn within(path: &str, folder: &str) -> bool {
    folder == "/"
        || path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// SQL range for "strictly beneath `folder`": `path >= 'F/' AND path < 'F0'`
/// (`0` follows `/` in byte order). Zone maps prune it.
fn beneath(folder: &str) -> (String, String) {
    if folder == "/" {
        ("/".to_string(), "0".to_string())
    } else {
        (format!("{folder}/"), format!("{folder}0"))
    }
}

/// The immediate children of `path` (default: the scan root) in the
/// snapshot on `conn`, leaving out everything inside `excluded`
/// (REQ-DSCR-002.A1). A path that is neither the scan root nor a directory
/// beneath it is not-found (exit 4, REQ-DSCR-003.A3). `capacity_bytes` is
/// supplied by the caller (it reads the live volume).
pub fn map(
    conn: &Connection,
    path: Option<&str>,
    excluded: &[String],
    capacity_bytes: u64,
) -> Result<MapView> {
    let root = crate::db::read_scan_meta(conn)
        .map(|m| normalize(&m.root))
        .ok_or_else(|| {
            DiskyError::new(
                ExitCode::NotFound,
                "snapshot has no scan root",
                "the snapshot carries no scan metadata; re-run `disky scan`",
            )
        })?;
    let folder = path.map_or_else(|| root.clone(), normalize);
    let listed = folder == root
        || (within(&folder, &root)
            && conn
                .query_row(
                    "SELECT count(*) FROM files WHERE path = ? AND is_dir",
                    [&folder],
                    |r| r.get::<_, i64>(0),
                )
                .map(|n| n > 0)?);
    if !listed {
        return Err(DiskyError::new(
            ExitCode::NotFound,
            "folder not in snapshot",
            format!("{folder} is not a folder in this snapshot (scan root: {root})"),
        )
        .into());
    }

    let (lo, hi) = beneath(&folder);
    let skip = lo.len() as i64; // chars before the child's name
    let mut sql = String::from(
        "SELECT split_part(substr(path, ? + 1), '/', 1) AS child,
                sum(CASE WHEN is_dir THEN 0 ELSE coalesce(physical_size, 0) END) AS bytes,
                bool_or(strpos(substr(path, ? + 1), '/') > 0) AS deeper
         FROM files WHERE path >= ? AND path < ?",
    );
    let mut params: Vec<Value> = vec![
        Value::BigInt(skip),
        Value::BigInt(skip),
        Value::Text(lo.clone()),
        Value::Text(hi),
    ];
    for e in excluded {
        let e = e.trim_end_matches('/');
        if e.is_empty() || within(&folder, e) {
            // The folder itself is excluded: nothing beneath it is shown.
            sql.push_str(" AND false");
            break;
        }
        sql.push_str(" AND NOT (path = ? OR (path >= ? AND path < ?))");
        params.push(Value::Text(e.to_string()));
        params.push(Value::Text(format!("{e}/")));
        params.push(Value::Text(format!("{e}0")));
    }
    sql.push_str(" GROUP BY child HAVING child <> '' ORDER BY bytes DESC, child");

    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let records = stmt
        .query_map(refs.as_slice(), |r| {
            let name: String = r.get(0)?;
            let bytes: Option<i128> = r.get(1)?;
            Ok(MapChild {
                path: format!("{lo}{name}"),
                name,
                bytes: bytes.unwrap_or(0).max(0) as u64,
                has_children: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(MapView {
        name: basename(&folder),
        total_bytes: records.iter().map(|c| c.bytes).sum(),
        path: folder,
        root,
        capacity_bytes,
        records,
        other: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(path, is_dir, physical)` rows under scan root `root`.
    fn snapshot(rows: &[(&str, bool, i64)], root: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        for (p, d, b) in rows {
            let name = p.rsplit('/').next().unwrap_or(p);
            conn.execute(
                "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth)
                 VALUES (?, ?, NULL, ?, ?, 0, ?, 0)",
                duckdb::params![p, name, b * 3, if *d { None } else { Some(*b) }, d],
            )
            .unwrap();
        }
        crate::db::write_scan_meta(&conn, root, 0, Some(1), true, 0, 0).unwrap();
        conn
    }

    fn sample() -> Connection {
        snapshot(
            &[
                ("/r", true, 0),
                ("/r/Users", true, 0),
                ("/r/Users/me", true, 0),
                ("/r/Users/me/a.bin", false, 700),
                ("/r/Users/me/b.bin", false, 50),
                ("/r/Apps", true, 0),
                ("/r/Apps/x.app", false, 300),
                ("/r/empty", true, 0),
                ("/r/file.txt", false, 9),
                ("/rx/outside", false, 999),
            ],
            "/r/",
        )
    }

    fn names(v: &MapView) -> Vec<(&str, u64, bool)> {
        v.records
            .iter()
            .map(|c| (c.name.as_str(), c.bytes, c.has_children))
            .collect()
    }

    #[test]
    fn map_lists_immediate_children_largest_first() {
        let v = map(&sample(), None, &[], 1_000).unwrap();
        assert_eq!(v.path, "/r");
        assert_eq!(v.name, "r");
        assert_eq!(v.root, "/r");
        assert_eq!(
            names(&v),
            [
                ("Users", 750, true),
                ("Apps", 300, true),
                ("file.txt", 9, false),
                ("empty", 0, false),
            ]
        );
        assert_eq!(v.records[0].path, "/r/Users");
        assert_eq!(v.total_bytes, 1_059, "physical, not logical");
        assert_eq!(v.capacity_bytes, 1_000);
    }

    #[test]
    fn map_drills_into_a_child_folder() {
        let v = map(&sample(), Some("/r/Users/"), &[], 1).unwrap();
        assert_eq!(v.path, "/r/Users");
        assert_eq!(names(&v), [("me", 750, true)]);
        let me = map(&sample(), Some("/r/Users/me"), &[], 1).unwrap();
        assert_eq!(names(&me), [("a.bin", 700, false), ("b.bin", 50, false)]);
    }

    #[test]
    fn map_outside_the_snapshot_is_not_found() {
        for p in ["/rx", "/r/missing", "/elsewhere", "/r/file.txt"] {
            let err = crate::exit::classify(map(&sample(), Some(p), &[], 1).unwrap_err());
            assert_eq!(err.code, ExitCode::NotFound, "{p}");
        }
    }

    #[test]
    fn map_leaves_out_excluded_folders() {
        let v = map(&sample(), None, &["/r/Users/me/".into()], 1).unwrap();
        assert_eq!(names(&v)[0], ("Apps", 300, true));
        assert!(v.records.iter().any(|c| c.name == "Users" && c.bytes == 0));
        let inside = map(&sample(), Some("/r/Users/me"), &["/r/Users".into()], 1).unwrap();
        assert!(inside.records.is_empty());
        // Components, not string prefixes: `/r/App` excludes nothing here.
        let v = map(&sample(), None, &["/r/App".into()], 1).unwrap();
        assert_eq!(v.total_bytes, 1_059);
    }

    /// A Windows drive root, stored `/`-separated (REQ-XPLAT-001.A3).
    #[test]
    fn drive_root_children_are_listed() {
        let conn = snapshot(
            &[
                ("C:/", true, 0),
                ("C:/Users", true, 0),
                ("C:/Users/me/f", false, 5),
                ("C:/Windows", true, 0),
                ("C:/Windows/x.dll", false, 3),
                ("D:/other", false, 99),
            ],
            "C:/",
        );
        for p in [None, Some("C:/"), Some("C:")] {
            let v = map(&conn, p, &[], 1).unwrap();
            assert_eq!(v.path, "C:", "{p:?}");
            assert_eq!(names(&v), [("Users", 5, true), ("Windows", 3, true)]);
            assert_eq!(v.records[0].path, "C:/Users");
        }
        let users = map(&conn, Some("C:/Users"), &[], 1).unwrap();
        assert_eq!(names(&users), [("me", 5, true)]);
    }

    #[test]
    fn map_of_the_filesystem_root() {
        let conn = snapshot(
            &[
                ("/", true, 0),
                ("/Users", true, 0),
                ("/Users/f", false, 5),
                ("/System", false, 2),
            ],
            "/",
        );
        let v = map(&conn, None, &[], 1).unwrap();
        assert_eq!((v.path.as_str(), v.name.as_str()), ("/", "/"));
        assert_eq!(names(&v), [("Users", 5, true), ("System", 2, false)]);
        assert_eq!(v.records[0].path, "/Users");
    }

    #[test]
    fn map_json_shape() {
        let v = map(&sample(), None, &[], 7).unwrap();
        let j = serde_json::to_value(&v).unwrap();
        assert_eq!(j["records"][0]["has_children"], true);
        assert!(j["records"][0]["bytes"].is_u64());
        assert_eq!(j["capacity_bytes"], 7);
    }

    /// A view of `n` children whose sizes are `sizes`, largest first.
    fn view_of(sizes: &[u64]) -> MapView {
        MapView {
            path: "/r".into(),
            name: "r".into(),
            root: "/r".into(),
            total_bytes: sizes.iter().sum(),
            capacity_bytes: 1,
            records: sizes
                .iter()
                .enumerate()
                .map(|(i, b)| MapChild {
                    name: format!("c{i}"),
                    path: format!("/r/c{i}"),
                    bytes: *b,
                    has_children: true,
                })
                .collect(),
            other: None,
        }
    }

    #[test]
    fn fold_keeps_the_largest_and_sums_the_rest_into_other() {
        // 30 children: the cap keeps 19, the 20th block is "Other".
        let sizes: Vec<u64> = (0..30).map(|i| 1_000 - i).collect();
        let v = view_of(&sizes).fold(20, 0.0);
        assert_eq!(v.records.len(), 19);
        let o = v.other.as_ref().unwrap();
        assert_eq!(o.count, 11);
        assert_eq!(o.bytes, sizes[19..].iter().sum::<u64>());
        // Nothing is lost: kept + other = the folder.
        assert_eq!(
            v.records.iter().map(|c| c.bytes).sum::<u64>() + o.bytes,
            v.total_bytes
        );
    }

    #[test]
    fn fold_moves_slivers_under_the_share_into_other() {
        let v = view_of(&[900, 80, 4, 3, 2]).fold(20, 0.005);
        // 0.5% of 989 rounds up to 5 bytes: 4, 3 and 2 are slivers.
        assert_eq!(names(&v).len(), 2);
        assert_eq!(v.other, Some(MapOther { count: 3, bytes: 9 }));
    }

    #[test]
    fn fold_leaves_a_small_folder_untouched() {
        let v = view_of(&[50, 40, 10]).fold(20, 0.005);
        assert_eq!(v.records.len(), 3);
        assert_eq!(v.other, None);
        // A single sliver is kept as itself, never renamed "Other".
        let v = view_of(&[1_000, 1]).fold(20, 0.005);
        assert_eq!(v.records.len(), 2);
        assert_eq!(v.other, None);
    }
}
