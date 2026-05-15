use anyhow::Result;
use duckdb::Connection;
use humansize::{format_size, BINARY};

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path:     String,
    pub name:     String,
    pub is_dir:   bool,
    pub size:     i64,
    pub depth:    usize,
    pub expanded: bool,
    pub loaded:   bool,
    pub children: Vec<DirEntry>,
}

impl DirEntry {
    pub fn size_str(&self) -> String {
        if self.size == 0 {
            "-".into()
        } else {
            format_size(self.size as u64, BINARY)
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.is_dir { "▶" } else { " " }
    }

    pub fn expanded_icon(&self) -> &'static str {
        if !self.is_dir { return " "; }
        if self.expanded { "▼" } else { "▶" }
    }
}

pub fn load_children(conn: &Connection, parent_path: &str, parent_depth: usize) -> Result<Vec<DirEntry>> {
    let child_depth = parent_depth + 1;
    let like_pat = format!("{}/%", parent_path);

    let mut stmt = conn.prepare(
        "SELECT name, path, is_dir, size
         FROM files
         WHERE depth = ? AND path LIKE ?
         ORDER BY is_dir DESC, size DESC
         LIMIT 500"
    )?;

    let mut children: Vec<DirEntry> = stmt.query_map(
        duckdb::params![child_depth as i32, like_pat],
        |row| Ok(DirEntry {
            name:     row.get(0)?,
            path:     row.get(1)?,
            is_dir:   row.get(2)?,
            size:     row.get(3)?,
            depth:    child_depth,
            expanded: false,
            loaded:   false,
            children: vec![],
        }),
    )?.flatten().collect();

    // compute accumulated sizes for dirs
    for entry in children.iter_mut() {
        if entry.is_dir {
            let acc_like = format!("{}/%", entry.path);
            let acc: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(size), 0) FROM files WHERE path LIKE ? AND is_dir = false",
                    duckdb::params![acc_like],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            entry.size = acc;
        }
    }

    // re-sort after computing dir sizes
    children.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(children)
}

pub fn load_root(conn: &Connection, root: &str) -> Result<DirEntry> {
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(size), 0) FROM files WHERE is_dir = false",
        [],
        |r| r.get(0),
    )?;

    let mut root_entry = DirEntry {
        path:     root.to_string(),
        name:     root.to_string(),
        is_dir:   true,
        size:     total,
        depth:    0,
        expanded: true,
        loaded:   true,
        children: vec![],
    };

    root_entry.children = load_children(conn, root, 0)?;
    Ok(root_entry)
}
