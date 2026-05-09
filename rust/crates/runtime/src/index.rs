#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::too_many_arguments, clippy::unused_self)]
use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use regex::Regex;
use sha2::{Sha256, Digest};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

#[derive(Clone)]
pub struct CodebaseIndex {
    path: PathBuf,
}

impl CodebaseIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let index = Self { path };
        index.init_schema()?;
        Ok(index)
    }

    fn connect(&self) -> Result<Connection, String> {
        Connection::open(&self.path).map_err(|e| e.to_string())
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                mtime INTEGER NOT NULL,
                hash TEXT NOT NULL
            )",
            [],
        ).map_err(|e| e.to_string())?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                line INTEGER NOT NULL,
                FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
            )",
            [],
        ).map_err(|e| e.to_string())?;
        
        Ok(())
    }

    pub fn update(&self, root: impl AsRef<Path>) -> Result<(), String> {
        let root = root.as_ref();
        let conn = self.connect()?;
        
        // Regexes for common symbols
        let re_rust_fn = Regex::new(r"fn\s+([a-zA-Z0-0_]+)\s*\(").map_err(|e| e.to_string())?;
        let re_rust_struct = Regex::new(r"struct\s+([a-zA-Z0-0_]+)").map_err(|e| e.to_string())?;
        let re_py_fn = Regex::new(r"def\s+([a-zA-Z0-0_]+)\s*\(").map_err(|e| e.to_string())?;
        let re_py_class = Regex::new(r"class\s+([a-zA-Z0-0_]+)").map_err(|e| e.to_string())?;

        for entry in WalkDir::new(root).into_iter().filter_map(std::result::Result::ok) {
            if entry.file_type().is_file() {
                let path = entry.path();
                let rel_path = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().to_string();
                
                if rel_path.contains(".klako") || rel_path.contains(".git") || rel_path.contains("target/") || rel_path.contains("node_modules/") {
                    continue;
                }

                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if !matches!(extension, "rs" | "py" | "js" | "ts" | "tsx") {
                    continue;
                }

                let mtime = entry.metadata().map_err(|e| e.to_string())?
                    .modified().map_err(|e| e.to_string())?
                    .duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?
                    .as_secs() as i64;
                
                let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let hash = format!("{:x}", Sha256::digest(content.as_bytes()));

                let existing: Option<(i64, String)> = conn.query_row(
                    "SELECT id, hash FROM files WHERE path = ?1",
                    params![rel_path],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional().map_err(|e| e.to_string())?;

                if let Some((id, old_hash)) = existing {
                    if old_hash == hash {
                        continue;
                    }
                    conn.execute("DELETE FROM symbols WHERE file_id = ?1", params![id]).map_err(|e| e.to_string())?;
                    conn.execute("UPDATE files SET mtime = ?1, hash = ?2 WHERE id = ?3", params![mtime, hash, id]).map_err(|e| e.to_string())?;
                    self.index_content(&conn, id, &content, &re_rust_fn, &re_rust_struct, &re_py_fn, &re_py_class)?;
                } else {
                    conn.execute("INSERT INTO files (path, mtime, hash) VALUES (?1, ?2, ?3)", params![rel_path, mtime, hash]).map_err(|e| e.to_string())?;
                    let id = conn.last_insert_rowid();
                    self.index_content(&conn, id, &content, &re_rust_fn, &re_rust_struct, &re_py_fn, &re_py_class)?;
                }
            }
        }

        Ok(())
    }

    fn index_content(&self, conn: &Connection, file_id: i64, content: &str, re_rust_fn: &Regex, re_rust_struct: &Regex, re_py_fn: &Regex, re_py_class: &Regex) -> Result<(), String> {
        for (idx, line) in content.lines().enumerate() {
            if let Some(cap) = re_rust_fn.captures(line) {
                self.insert_symbol(conn, file_id, &cap[1], "function", idx + 1)?;
            } else if let Some(cap) = re_rust_struct.captures(line) {
                self.insert_symbol(conn, file_id, &cap[1], "struct", idx + 1)?;
            } else if let Some(cap) = re_py_fn.captures(line) {
                self.insert_symbol(conn, file_id, &cap[1], "function", idx + 1)?;
            } else if let Some(cap) = re_py_class.captures(line) {
                self.insert_symbol(conn, file_id, &cap[1], "class", idx + 1)?;
            }
        }
        Ok(())
    }

    fn insert_symbol(&self, conn: &Connection, file_id: i64, name: &str, kind: &str, line: usize) -> Result<(), String> {
        conn.execute(
            "INSERT INTO symbols (file_id, name, kind, line) VALUES (?1, ?2, ?3, ?4)",
            params![file_id, name, kind, line as i64],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn query_symbol(&self, name: &str) -> Result<Vec<(String, Symbol)>, String> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT f.path, s.name, s.kind, s.line 
             FROM symbols s 
             JOIN files f ON s.file_id = f.id 
             WHERE s.name = ?1"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map(params![name], |row| {
            Ok((
                row.get(0)?,
                Symbol {
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    line: row.get::<_, i64>(3)? as usize,
                }
            ))
        }).map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| e.to_string())?);
        }
        Ok(results)
    }
}

use notify::{RecursiveMode, Watcher};

pub struct IndexerDaemon {
    index: CodebaseIndex,
    root: PathBuf,
}

impl IndexerDaemon {
    #[must_use] 
    pub fn new(index: CodebaseIndex, root: PathBuf) -> Self {
        Self { index, root }
    }

    pub async fn run(&self) -> Result<(), String> {
        let index = self.index.clone();
        let root = self.root.clone();

        // Initial full scan
        let i_init = index.clone();
        let r_init = root.clone();
        tokio::task::spawn_blocking(move || {
            i_init.update(&r_init)
        }).await.map_err(|e| e.to_string())??;

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(_event) = res {
                let _ = tx.try_send(());
            }
        }).map_err(|e| e.to_string())?;

        watcher.watch(&root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;

        // Throttled update loop
        loop {
            let _ = rx.recv().await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            while rx.try_recv().is_ok() {}
            
            let i = index.clone();
            let r = root.clone();
            let _ = tokio::task::spawn_blocking(move || {
                i.update(&r)
            }).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_codebase_index_lifecycle() {
        let dir = tempdir().unwrap();
        let index_path = dir.path().join("index.db");
        let index = CodebaseIndex::open(&index_path).unwrap();

        // Create mock files
        let src_dir = dir.path().join("src");
        std::fs::create_dir(&src_dir).unwrap();
        let main_rs = src_dir.join("main.rs");
        std::fs::write(&main_rs, "fn main() {}\nstruct Config {}").unwrap();

        index.update(&dir).unwrap();

        let results = index.query_symbol("main").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "src/main.rs");
        assert_eq!(results[0].1.kind, "function");

        let results = index.query_symbol("Config").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.kind, "struct");
        
        // Test incremental update
        std::fs::write(&main_rs, "fn main() {}\nstruct NewConfig {}").unwrap();
        index.update(&dir).unwrap();
        
        let results = index.query_symbol("Config").unwrap();
        assert_eq!(results.len(), 0);
        
        let results = index.query_symbol("NewConfig").unwrap();
        assert_eq!(results.len(), 1);
    }
}
