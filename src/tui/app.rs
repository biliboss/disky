use super::tree::{load_children, DirEntry};
use anyhow::Result;
use duckdb::Connection;

pub struct App {
    pub db_path: String,
    pub root: DirEntry,
    pub flat: Vec<FlatItem>,
    pub selected: usize,
    pub status: String,
    pub quitting: bool,
}

#[derive(Clone)]
pub struct FlatItem {
    pub depth: usize,
    pub is_dir: bool,
    pub path: String,
    pub name: String,
    pub size: i64,
    pub expanded: bool,
}

impl App {
    pub fn new(db_path: String, root: DirEntry) -> Self {
        let mut app = Self {
            db_path,
            root,
            flat: vec![],
            selected: 0,
            status: String::new(),
            quitting: false,
        };
        app.rebuild_flat();
        app
    }

    pub fn rebuild_flat(&mut self) {
        self.flat.clear();
        flatten(&self.root, &mut self.flat);
        // skip root itself, start from children
        if !self.flat.is_empty() {
            self.flat.remove(0);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.flat.len() {
            self.selected += 1;
        }
    }

    pub fn toggle_expand(&mut self, conn: &Connection) -> Result<()> {
        let Some(item) = self.flat.get(self.selected).cloned() else {
            return Ok(());
        };
        if !item.is_dir {
            return Ok(());
        }

        self.toggle_in_tree(&item.path, conn)?;
        let sel_path = item.path.clone();
        self.rebuild_flat();
        // restore selection to same path
        if let Some(idx) = self.flat.iter().position(|f| f.path == sel_path) {
            self.selected = idx;
        }
        Ok(())
    }

    fn toggle_in_tree(&mut self, path: &str, conn: &Connection) -> Result<()> {
        toggle_node(&mut self.root, path, conn)
    }

    pub fn selected_path(&self) -> Option<String> {
        self.flat.get(self.selected).map(|f| f.path.clone())
    }

    pub fn copy_path(&mut self) -> Result<()> {
        if let Some(path) = self.selected_path() {
            let mut ctx = arboard::Clipboard::new()?;
            ctx.set_text(&path)?;
            self.status = format!("Copied: {}", path);
        }
        Ok(())
    }

    pub fn open_finder(&mut self) -> Result<()> {
        if let Some(path) = self.selected_path() {
            std::process::Command::new("open").arg(&path).spawn()?;
            self.status = format!("Opened: {}", path);
        }
        Ok(())
    }
}

fn flatten(node: &DirEntry, out: &mut Vec<FlatItem>) {
    out.push(FlatItem {
        depth: node.depth,
        is_dir: node.is_dir,
        path: node.path.clone(),
        name: node.name.clone(),
        size: node.size,
        expanded: node.expanded,
    });
    if node.expanded {
        for child in &node.children {
            flatten(child, out);
        }
    }
}

fn toggle_node(node: &mut DirEntry, path: &str, conn: &Connection) -> Result<()> {
    if node.path == path {
        if node.expanded {
            node.expanded = false;
        } else {
            if !node.loaded {
                node.children = load_children(conn, &node.path, node.depth)?;
                node.loaded = true;
            }
            node.expanded = true;
        }
        return Ok(());
    }
    for child in node.children.iter_mut() {
        if path.starts_with(&child.path) {
            toggle_node(child, path, conn)?;
            break;
        }
    }
    Ok(())
}
