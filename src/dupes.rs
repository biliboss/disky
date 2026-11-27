//! Duplicate files (REQ-DSCR-005): regular files whose contents are
//! byte-identical, grouped, with the bytes a clean-up would reclaim.
//!
//! `disky dupes` and the GUI's Duplicates screen read the same [`dupes`]
//! call. It narrows the snapshot in four steps, cheapest first:
//!
//! 1. **Size** — one query keeps files whose logical size occurs at least
//!    twice, scoped by excluded folders and skip-recently-used exactly like
//!    the Large files list (REQ-DSCR-002).
//! 2. **Re-check** — each candidate is `lstat`ed: a file that vanished,
//!    stopped being a regular file or changed size/mtime since the scan is
//!    dropped (A3), and hard links to one inode count once (A4).
//! 3. **Prefix hash** — the first [`PREFIX_BYTES`] of each file split every
//!    size bucket, so most non-duplicates never get read in full.
//! 4. **Full hash** — a full-content BLAKE3 hash decides (A1). Names never
//!    take part in the comparison.
//!
//! @implements REQ-DSCR-005

use anyhow::Result;
use duckdb::types::Value;
use duckdb::Connection;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::Path;

use crate::cleanup::ScanContext;

/// Files smaller than this are not compared unless `--min-size` says so:
/// 1 MB, decimal (A1).
pub const DEFAULT_MIN_BYTES: u64 = 1_000_000;

/// Bytes read from the start of each candidate for the prefilter: 64 KiB.
pub const PREFIX_BYTES: u64 = 64 * 1024;

/// Read buffer for the full hash: 1 MiB, streamed.
const CHUNK_BYTES: usize = 1 << 20;

/// One copy of a duplicated file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DupeCopy {
    /// Absolute path.
    pub path: String,
    /// The parent folder for display, with the home directory as `~`.
    pub location: String,
    /// Modification time (RFC 3339 UTC), or null when unknown.
    pub mtime: Option<String>,
}

/// Files with byte-identical contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DupeGroup {
    /// Basename of the first copy.
    pub name: String,
    /// Physical bytes of each copy (the largest across copies, so a sparse
    /// copy never under-counts).
    pub bytes: u64,
    /// Every copy, oldest modification time first (unknown last), then by
    /// path. Always two or more.
    pub copies: Vec<DupeCopy>,
}

impl DupeGroup {
    /// Bytes freed by keeping one copy: `bytes × (copies − 1)`.
    pub fn reclaimable(&self) -> u64 {
        self.bytes * (self.copies.len().saturating_sub(1) as u64)
    }
}

/// What `disky dupes` prints and the Duplicates screen renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DupesView {
    /// Smallest logical size compared.
    pub min_size: u64,
    /// Groups in `records`.
    pub groups: usize,
    /// Copies beyond the first, over every group.
    pub extra_copies: usize,
    /// Sum of every group's reclaimable bytes.
    pub reclaimable_bytes: u64,
    /// Largest reclaimable first (ties by name, then first path).
    pub records: Vec<DupeGroup>,
}

impl DupesView {
    /// Every copy's path: the list a Trash request is checked against
    /// (REQ-DSCR-006.A2).
    pub fn listed_paths(&self) -> Vec<String> {
        self.records
            .iter()
            .flat_map(|g| g.copies.iter().map(|c| c.path.clone()))
            .collect()
    }

    /// Each group's paths, so a request that would remove every copy of a
    /// group is refused (REQ-DSCR-006.A3).
    pub fn group_paths(&self) -> Vec<Vec<String>> {
        self.records
            .iter()
            .map(|g| g.copies.iter().map(|c| c.path.clone()).collect())
            .collect()
    }
}

/// A file that might have a twin, as the snapshot recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub path: String,
    /// Logical size: what equal contents share.
    pub size: u64,
    pub physical: u64,
    pub mtime: Option<i64>,
}

/// Content hash.
pub(crate) type Digest = [u8; 32];

/// The duplicate groups among regular files of at least `min_size` logical
/// bytes in the snapshot on `conn`, scoped by `ctx` (excluded folders and
/// skip-recently-used, REQ-DSCR-002.A1/A2).
pub fn dupes(conn: &Connection, min_size: u64, ctx: &ScanContext) -> Result<DupesView> {
    let cands = recheck(candidates(conn, min_size, ctx)?);
    let groups = group(cands, prefix_hash, full_hash, ctx.home.as_deref());
    Ok(view(min_size, groups))
}

/// Step 1: files whose size occurs at least twice in scope, by size then
/// path.
pub(crate) fn candidates(
    conn: &Connection,
    min_size: u64,
    ctx: &ScanContext,
) -> Result<Vec<Candidate>> {
    let mut scoped = String::from(
        "SELECT path, size, physical_size, mtime FROM files
         WHERE NOT is_dir AND size >= ?",
    );
    let mut params: Vec<Value> = vec![Value::BigInt(min_size.min(i64::MAX as u64) as i64)];
    crate::large::scope_clause(ctx, &mut scoped, &mut params);
    let sql = format!(
        "WITH c AS ({scoped})
         SELECT * FROM c
         WHERE size IN (SELECT size FROM c GROUP BY size HAVING count(*) > 1)
         ORDER BY size, path"
    );
    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            let size = r.get::<_, i64>(1)?.max(0) as u64;
            Ok(Candidate {
                path: r.get(0)?,
                size,
                physical: r
                    .get::<_, Option<i64>>(2)?
                    .map_or(size, |p| p.max(0) as u64),
                mtime: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Step 2: keep candidates that are still regular files of the recorded
/// size and mtime (A3), and only the first path of each inode (A4).
pub(crate) fn recheck(cands: Vec<Candidate>) -> Vec<Candidate> {
    let checked: Vec<Option<(Candidate, (u64, u64))>> = cands
        .into_par_iter()
        .map(|c| {
            let meta = std::fs::symlink_metadata(&c.path).ok()?;
            if !meta.is_file() || meta.len() != c.size {
                return None;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            if c.mtime.is_some() && mtime != c.mtime {
                return None;
            }
            let id = inode(Path::new(&c.path), &meta);
            Some((c, id))
        })
        .collect();
    let mut seen = HashSet::new();
    checked
        .into_iter()
        .flatten()
        .filter(|(_, ino)| seen.insert(*ino))
        .map(|(c, _)| c)
        .collect()
}

/// The file's identity: two paths with one identity are hard links to one
/// file, which frees nothing when one of them is trashed (A4).
#[cfg(unix)]
fn inode(_path: &Path, meta: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), meta.ino())
}

/// The volume serial number and file index (`GetFileInformationByHandle`;
/// std's `MetadataExt::file_index` is unstable). Unreadable → a unique id,
/// so the path is its own file.
#[cfg(windows)]
fn inode(path: &Path, _meta: &std::fs::Metadata) -> (u64, u64) {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    #[derive(Default)]
    struct ByHandleFileInformation {
        attributes: u32,
        times: [u32; 6],
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    extern "system" {
        fn GetFileInformationByHandle(
            file: std::os::windows::io::RawHandle,
            info: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let Ok(f) = std::fs::File::open(path) else {
        return unique_id();
    };
    let mut info = ByHandleFileInformation::default();
    // SAFETY: `f` is an open handle and `info` matches
    // BY_HANDLE_FILE_INFORMATION's layout (13 DWORDs).
    if unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) } == 0 {
        return unique_id();
    }
    (
        u64::from(info.volume_serial),
        (u64::from(info.index_high) << 32) | u64::from(info.index_low),
    )
}

#[cfg(not(any(unix, windows)))]
fn inode(_path: &Path, _meta: &std::fs::Metadata) -> (u64, u64) {
    unique_id()
}

/// An id no other file has: the path counts as its own file.
#[cfg(not(unix))]
fn unique_id() -> (u64, u64) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    (
        u64::MAX,
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    )
}

/// BLAKE3 of the first [`PREFIX_BYTES`] of `path`. Unreadable → `None`.
pub(crate) fn prefix_hash(path: &Path) -> Option<Digest> {
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(PREFIX_BYTES as usize);
    f.take(PREFIX_BYTES).read_to_end(&mut buf).ok()?;
    Some(*blake3::hash(&buf).as_bytes())
}

/// BLAKE3 of the whole of `path`, streamed in 1 MiB chunks. Unreadable →
/// `None`.
pub(crate) fn full_hash(path: &Path) -> Option<Digest> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK_BYTES];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update_rayon(&buf[..n]);
    }
    Some(*hasher.finalize().as_bytes())
}

/// Split each bucket by `key` (computed in parallel across every bucket),
/// keeping sub-buckets of two or more. A `None` key drops that file.
fn split_by<K: Ord + Send>(
    buckets: Vec<Vec<Candidate>>,
    key: impl Fn(&Candidate) -> Option<K> + Sync,
) -> Vec<Vec<Candidate>> {
    let keyed: Vec<(usize, K, Candidate)> = buckets
        .into_iter()
        .enumerate()
        .flat_map(|(i, b)| b.into_iter().map(move |c| (i, c)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(i, c)| key(&c).map(|k| (i, k, c)))
        .collect();
    let mut out: BTreeMap<(usize, K), Vec<Candidate>> = BTreeMap::new();
    for (i, k, c) in keyed {
        out.entry((i, k)).or_default().push(c);
    }
    out.into_values().filter(|b| b.len() >= 2).collect()
}

/// Steps 3–4 over re-checked candidates: bucket by size, split by
/// `prefix`, then by `full` — the deciding comparison (A1). Each surviving
/// bucket becomes a group, copies oldest mtime first.
pub(crate) fn group(
    cands: Vec<Candidate>,
    prefix: impl Fn(&Path) -> Option<Digest> + Sync,
    full: impl Fn(&Path) -> Option<Digest> + Sync,
    home: Option<&Path>,
) -> Vec<DupeGroup> {
    let mut by_size: BTreeMap<u64, Vec<Candidate>> = BTreeMap::new();
    for c in cands {
        by_size.entry(c.size).or_default().push(c);
    }
    let buckets: Vec<Vec<Candidate>> = by_size.into_values().filter(|b| b.len() >= 2).collect();
    let buckets = split_by(buckets, |c| prefix(Path::new(&c.path)));
    let buckets = split_by(buckets, |c| full(Path::new(&c.path)));
    buckets
        .into_iter()
        .map(|mut b| {
            b.sort_by(|x, y| {
                (x.mtime.is_none(), x.mtime, &x.path).cmp(&(y.mtime.is_none(), y.mtime, &y.path))
            });
            let bytes = b.iter().map(|c| c.physical).max().unwrap_or(0);
            let copies: Vec<DupeCopy> = b
                .into_iter()
                .map(|c| {
                    let parent = Path::new(&c.path).parent().unwrap_or(Path::new("/"));
                    DupeCopy {
                        location: crate::prefs::abbreviate_home(parent, home),
                        mtime: crate::query::rfc3339(c.mtime),
                        path: c.path,
                    }
                })
                .collect();
            DupeGroup {
                name: basename(&copies[0].path),
                bytes,
                copies,
            }
        })
        .collect()
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Sort by reclaimable bytes, largest first, and total (A2).
fn view(min_size: u64, mut records: Vec<DupeGroup>) -> DupesView {
    records.sort_by(|a, b| {
        b.reclaimable()
            .cmp(&a.reclaimable())
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.copies[0].path.cmp(&b.copies[0].path))
    });
    DupesView {
        min_size,
        groups: records.len(),
        extra_copies: records.iter().map(|g| g.copies.len() - 1).sum(),
        reclaimable_bytes: records.iter().map(DupeGroup::reclaimable).sum(),
        records,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const DAY: i64 = 86_400;

    /// Write `bytes` to `dir/rel` with mtime `days` ago; return its path.
    fn file(dir: &Path, rel: &str, bytes: &[u8], days: i64) -> String {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs((days * DAY) as u64);
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(t)
            .unwrap();
        crate::platform::to_slash(&p)
    }

    /// An in-memory snapshot recording each path as it is on disk now.
    fn snapshot(paths: &[&str]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        for p in paths {
            let meta = std::fs::symlink_metadata(p).unwrap();
            let mtime = meta
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            conn.execute(
                "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth)
                 VALUES (?, ?, NULL, ?, ?, ?, ?, 0)",
                duckdb::params![
                    p,
                    basename(p),
                    meta.len() as i64,
                    meta.len() as i64,
                    mtime,
                    meta.is_dir()
                ],
            )
            .unwrap();
        }
        conn
    }

    fn run(paths: &[&str], min: u64) -> DupesView {
        dupes(&snapshot(paths), min, &ScanContext::default()).unwrap()
    }

    fn copy_paths(g: &DupeGroup) -> Vec<&str> {
        g.copies.iter().map(|c| c.path.as_str()).collect()
    }

    #[test]
    fn same_content_groups_name_alone_never_does() {
        let t = tempfile::tempdir().unwrap();
        let a = file(t.path(), "x/report.pdf", b"AAAA", 10);
        let b = file(t.path(), "y/report.pdf", b"BBBB", 10); // same name and size
        let c = file(t.path(), "z/copy of a.pdf", b"AAAA", 5);
        let v = run(&[&a, &b, &c], 1);
        assert_eq!(v.groups, 1, "only identical contents group");
        assert_eq!(copy_paths(&v.records[0]), [a.as_str(), c.as_str()]);
        assert_eq!(v.records[0].name, "report.pdf", "first copy's basename");
        assert_eq!(v.records[0].bytes, 4);
    }

    #[test]
    fn below_min_size_ignored() {
        assert_eq!(DEFAULT_MIN_BYTES, 1_000_000, "1 MB, decimal");
        let t = tempfile::tempdir().unwrap();
        let small = vec![1u8; 999_999];
        let big = vec![2u8; 1_000_000];
        let s1 = file(t.path(), "s1", &small, 10);
        let s2 = file(t.path(), "s2", &small, 10);
        let b1 = file(t.path(), "b1", &big, 10);
        let b2 = file(t.path(), "b2", &big, 10);
        let v = run(&[&s1, &s2, &b1, &b2], DEFAULT_MIN_BYTES);
        assert_eq!(v.groups, 1);
        assert_eq!(copy_paths(&v.records[0]), [b1.as_str(), b2.as_str()]);
        assert_eq!(v.min_size, DEFAULT_MIN_BYTES);
        assert_eq!(run(&[&s1, &s2, &b1, &b2], 1).groups, 2);
    }

    #[test]
    fn copies_ordered_oldest_mtime_first() {
        let t = tempfile::tempdir().unwrap();
        let new = file(t.path(), "a/new.bin", b"same", 1);
        let old = file(t.path(), "b/old.bin", b"same", 300);
        let mid = file(t.path(), "c/mid.bin", b"same", 40);
        let v = run(&[&new, &old, &mid], 1);
        assert_eq!(
            copy_paths(&v.records[0]),
            [old.as_str(), mid.as_str(), new.as_str()]
        );
        assert_eq!(v.records[0].name, "old.bin");
        assert!(v.records[0].copies[0].mtime.is_some());
    }

    #[test]
    fn groups_sorted_by_reclaimable_with_totals() {
        let t = tempfile::tempdir().unwrap();
        // 3 × 10 bytes → 20 reclaimable; 2 × 30 bytes → 30 reclaimable.
        let ten: Vec<String> = (0..3)
            .map(|i| file(t.path(), &format!("ten{i}"), &[9u8; 10], 10))
            .collect();
        let thirty: Vec<String> = (0..2)
            .map(|i| file(t.path(), &format!("thirty{i}"), &[3u8; 30], 10))
            .collect();
        let all: Vec<&str> = ten.iter().chain(&thirty).map(String::as_str).collect();
        let v = run(&all, 1);
        assert_eq!(v.groups, 2);
        assert_eq!(v.records[0].name, "thirty0", "30 reclaimable first");
        assert_eq!(v.records[0].reclaimable(), 30);
        assert_eq!(v.records[1].reclaimable(), 20);
        assert_eq!(v.extra_copies, 3);
        assert_eq!(v.reclaimable_bytes, 50);
        assert_eq!(v.listed_paths().len(), 5);
        assert_eq!(v.group_paths()[1].len(), 3);
    }

    #[test]
    fn changed_or_vanished_file_dropped_and_group_below_two_omitted() {
        let t = tempfile::tempdir().unwrap();
        let a = file(t.path(), "a.bin", b"twin", 10);
        let b = file(t.path(), "b.bin", b"twin", 10);
        let c = file(t.path(), "c.bin", b"twin", 10);
        let p = file(t.path(), "p.bin", b"pair", 10);
        let q = file(t.path(), "q.bin", b"pair", 10);
        let conn = snapshot(&[&a, &b, &c, &p, &q]);
        std::fs::remove_file(&c).unwrap(); // vanished
        std::fs::write(&q, b"PAIR").unwrap(); // changed since the scan
        let v = dupes(&conn, 1, &ScanContext::default()).unwrap();
        assert_eq!(v.groups, 1, "the p/q group fell below two copies");
        assert_eq!(copy_paths(&v.records[0]), [a.as_str(), b.as_str()]);
    }

    #[test]
    fn hard_links_count_once() {
        let t = tempfile::tempdir().unwrap();
        let a = file(t.path(), "a.bin", b"linked", 10);
        let link = t.path().join("link.bin");
        std::fs::hard_link(&a, &link).unwrap();
        let link = crate::platform::to_slash(&link);
        assert_eq!(run(&[&a, &link], 1).groups, 0, "one inode is one file");
        let real = file(t.path(), "real.bin", b"linked", 10);
        let v = run(&[&a, &link, &real], 1);
        assert_eq!(v.groups, 1);
        assert_eq!(copy_paths(&v.records[0]), [a.as_str(), real.as_str()]);
    }

    #[test]
    fn excluded_folder_left_out() {
        let t = tempfile::tempdir().unwrap();
        let a = file(t.path(), "keep/a.bin", b"same", 100);
        let b = file(t.path(), "keepx/b.bin", b"same", 100);
        let c = file(t.path(), "other/c.bin", b"same", 100);
        let conn = snapshot(&[&a, &b, &c]);
        let ctx = ScanContext {
            excluded: vec![crate::platform::to_slash(&t.path().join("keep"))],
            ..Default::default()
        };
        let v = dupes(&conn, 1, &ctx).unwrap();
        assert_eq!(
            copy_paths(&v.records[0]),
            [b.as_str(), c.as_str()],
            "components, not prefixes"
        );
    }

    #[test]
    fn recently_modified_left_out_when_skip_recently_used() {
        let t = tempfile::tempdir().unwrap();
        let old1 = file(t.path(), "old1", b"same", 100);
        let old2 = file(t.path(), "old2", b"same", 90);
        let fresh = file(t.path(), "fresh", b"same", 2);
        let conn = snapshot(&[&old1, &old2, &fresh]);
        let now = chrono::Utc::now().timestamp();
        let ctx = ScanContext {
            recent_cutoff: Some(now - 30 * DAY),
            ..Default::default()
        };
        let v = dupes(&conn, 1, &ctx).unwrap();
        assert_eq!(copy_paths(&v.records[0]), [old1.as_str(), old2.as_str()]);
        let off = dupes(&conn, 1, &ScanContext::default()).unwrap();
        assert_eq!(off.records[0].copies.len(), 3, "off: recent files included");
    }

    fn cand(path: &str, size: u64) -> Candidate {
        Candidate {
            path: path.into(),
            size,
            physical: size,
            mtime: Some(0),
        }
    }

    #[test]
    fn prefix_prefilter_spares_the_full_hash() {
        // Fake hashes keyed on the path's first letter.
        let full_calls = AtomicUsize::new(0);
        let prefix = |p: &Path| {
            let s = p.to_string_lossy();
            let mut d = [0u8; 32];
            d[0] = s.as_bytes()[1]; // "/a…" → 'a'
            Some(d)
        };
        let full = |p: &Path| {
            full_calls.fetch_add(1, Ordering::Relaxed);
            let s = p.to_string_lossy();
            let mut d = [0u8; 32];
            d[0] = s.as_bytes()[1];
            d[1] = u8::from(s.ends_with("odd"));
            Some(d)
        };
        let cands = vec![
            cand("/a1", 100),
            cand("/a2", 100),
            cand("/a3odd", 100), // same prefix, different full content
            cand("/b1", 100),    // unique prefix: never fully hashed
            cand("/c1", 200),    // unique size: never hashed at all
        ];
        let groups = group(cands, prefix, full, None);
        assert_eq!(groups.len(), 1);
        assert_eq!(copy_paths(&groups[0]), ["/a1", "/a2"]);
        assert_eq!(
            full_calls.load(Ordering::Relaxed),
            3,
            "only the colliding prefix bucket is fully hashed"
        );
    }

    #[test]
    fn unreadable_file_is_dropped() {
        let groups = group(
            vec![cand("/x", 5), cand("/y", 5), cand("/z", 5)],
            |_| Some([0; 32]),
            |p: &Path| (p != Path::new("/z")).then_some([1; 32]),
            None,
        );
        assert_eq!(copy_paths(&groups[0]), ["/x", "/y"]);
    }

    #[test]
    fn full_hash_decides_past_the_prefix() {
        let t = tempfile::tempdir().unwrap();
        let mut one = vec![0u8; (PREFIX_BYTES as usize) * 2];
        let mut two = one.clone();
        one[PREFIX_BYTES as usize + 1] = 1;
        two[PREFIX_BYTES as usize + 1] = 2;
        let a = file(t.path(), "a", &one, 10);
        let b = file(t.path(), "b", &two, 10);
        let c = file(t.path(), "c", &one, 10);
        assert_eq!(
            prefix_hash(Path::new(&a)),
            prefix_hash(Path::new(&b)),
            "same first 64 KiB"
        );
        let v = run(&[&a, &b, &c], 1);
        assert_eq!(copy_paths(&v.records[0]), [a.as_str(), c.as_str()]);
        assert_eq!(full_hash(&PathBuf::from("/no/such/file")), None);
    }

    #[test]
    fn home_shown_as_tilde_in_location() {
        let groups = group(
            vec![
                cand("/u/Pictures/a.heic", 5),
                cand("/u/Downloads/a.heic", 5),
            ],
            |_| Some([0; 32]),
            |_| Some([0; 32]),
            Some(Path::new("/u")),
        );
        let locs: Vec<&str> = groups[0]
            .copies
            .iter()
            .map(|c| c.location.as_str())
            .collect();
        assert_eq!(locs, ["~/Downloads", "~/Pictures"]);
    }
}
