use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct WorkspaceCheckpoint {
    workspace_root: PathBuf,
    git_dir: PathBuf,
    ignore_file: PathBuf,
}

impl WorkspaceCheckpoint {
    pub async fn new(workspace_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let klako_dir = workspace_root.join(".klako");
        let git_dir = klako_dir.join("shadow.git");
        let ignore_file = klako_dir.join("shadow_ignore");

        let manager = Self {
            workspace_root,
            git_dir,
            ignore_file,
        };
        manager.init_if_needed().await?;
        Ok(manager)
    }

    /// Constructs a git command safely bound to the disjoint shadow repository,
    /// explicitly isolating any operations from the user's root `.git/` folder.
    fn git(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.env("GIT_DIR", &self.git_dir)
            .env("GIT_WORK_TREE", &self.workspace_root)
            // Prevent global config and user aliases from interfering
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            // Apply the local isolation exclusions to avoid blowing out I/O
            .arg("-c")
            .arg(format!("core.excludesFile={}", self.ignore_file.display()));
        cmd
    }

    async fn with_lock<F, Fut, R>(&self, f: F) -> std::io::Result<R>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = std::io::Result<R>> + Send,
        R: Send + 'static,
    {
        use fs2::FileExt;
        let lock_file_path = self.workspace_root.join(".klako/shadow.lock");
        let _ = std::fs::create_dir_all(self.workspace_root.join(".klako"));
        
        // Use spawn_blocking for the blocking lock acquisition
        let lock_file = tokio::task::spawn_blocking(move || {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .open(&lock_file_path)?;
            file.lock_exclusive()?;
            Ok::<std::fs::File, std::io::Error>(file)
        }).await.map_err(std::io::Error::other)??;

        let res = f().await;
        
        let _ = tokio::task::spawn_blocking(move || {
            let _ = lock_file.unlock();
        }).await;
        
        res
    }

    async fn init_if_needed(&self) -> std::io::Result<()> {
        let klako_dir = self.workspace_root.join(".klako");
        if !klako_dir.exists() {
            std::fs::create_dir_all(&klako_dir)?;
        }

        // Automatically shield high volume dependencies to prevent disk indexing explosions
        let ignore_content = ".klako\n.git\ntarget/\nnode_modules/\nvenv/\n.venv/\n";
        std::fs::write(&self.ignore_file, ignore_content)?;

        if !self.git_dir.exists() {
            std::fs::create_dir_all(&self.git_dir)?;
            let mut init_cmd = Command::new("git");
            let status = init_cmd
                .current_dir(&self.workspace_root)
                .args(["init", "--bare", "--initial-branch=main"])
                .arg(&self.git_dir)
                .status()
                .await?;
                
            if !status.success() {
                return Err(std::io::Error::other(
                    "Failed to initialize shadow git repo",
                ));
            }
            
            // Perform an initial empty commit so there's always a valid HEAD
            let mut commit_cmd = self.git();
            let commit_status = commit_cmd
                .current_dir(&self.workspace_root)
                .args(["commit", "--allow-empty", "-m", "Initial Bound"])
                .status()
                .await?;
                
            if !commit_status.success() {
                 return Err(std::io::Error::other(
                    "Failed to generate valid HEAD",
                ));
            }
        }
        Ok(())
    }

    /// Takes an instantaneous delta snapshot into the shadow tracker
    pub async fn snapshot(&self) -> std::io::Result<()> {
        let git_dir = self.git_dir.clone();
        let workspace_root = self.workspace_root.clone();
        let ignore_file = self.ignore_file.clone();
        
        self.with_lock(move || async move {
            let mut cmd = Command::new("git");
            cmd.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));
            
            let add_status = cmd.args(["add", "-A"]).status().await?;
            if !add_status.success() {
                return Err(std::io::Error::other(
                    "Failed to index workspace during snapshot",
                ));
            }

            let mut cmd = Command::new("git");
            cmd.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));

            let _ = cmd
                .args(["commit", "--allow-empty", "-m", "Auto-Checkpoint"])
                .status()
                .await?;

            Ok(())
        }).await
    }

    /// Returns the current unstaged diff in Lore format.
    pub async fn get_current_diff(&self) -> std::io::Result<String> {
        let git_dir = self.git_dir.clone();
        let workspace_root = self.workspace_root.clone();
        let ignore_file = self.ignore_file.clone();

        self.with_lock(move || async move {
            let mut cmd = Command::new("git");
            cmd.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));

            let output = cmd.args(["diff", "HEAD"]).output().await?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }).await
    }

    /// Executes literal instantaneous Undo dropping the user's workspace back to the `snapshot()` tracker.
    pub async fn restore(&self) -> std::io::Result<()> {
        self.restore_commits(0).await
    }

    /// Restores the workspace by rewinding a specific number of snapshot commits.
    pub async fn restore_commits(&self, rewinds: usize) -> std::io::Result<()> {
        let git_dir = self.git_dir.clone();
        let workspace_root = self.workspace_root.clone();
        let ignore_file = self.ignore_file.clone();

        self.with_lock(move || async move {
            let mut cmd_base = Command::new("git");
            cmd_base.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));

            // Discard any untracked untamed trash generated
            let mut clean_cmd = Command::new("git");
            clean_cmd.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));
            
            let clean_status = clean_cmd.args(["clean", "-df"]).status().await?;
            if !clean_status.success() {
                return Err(std::io::Error::other(
                    "Failed to clean untracked files during restore",
                ));
            }

            let mut reset_cmd = Command::new("git");
            reset_cmd.current_dir(&workspace_root)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &workspace_root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .arg("-c")
                .arg(format!("core.excludesFile={}", ignore_file.display()));

            let target = format!("HEAD~{rewinds}");
            let reset_status = reset_cmd.args(["reset", "--hard", &target]).status().await?;
            if !reset_status.success() {
                return Err(std::io::Error::other(
                    "Failed to hard reset during restore",
                ));
            }

            Ok(())
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_workspace_checkpoint_snapshot_and_restore() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        let manager = WorkspaceCheckpoint::new(root).await.expect("new manager");

        // Write a known file out
        let file_path = root.join("hello.txt");
        std::fs::write(&file_path, b"pristine content").unwrap();

        // Snapshot it!
        manager.snapshot().await.expect("snapshot pristine content");

        // Mutate the workspace hallucinating garbage
        std::fs::write(&file_path, b"utter garbage hallucination").unwrap();
        let garbage_path = root.join("garbage.txt");
        std::fs::write(&garbage_path, b"some random logs").unwrap();

        // Restore instantly back to clean
        manager.restore().await.expect("restore process");

        // Verify bounds
        let restored_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored_content, "pristine content");

        // The garbage file should be wiped by the clean -df hook
        assert!(!garbage_path.exists());
    }
    
    #[tokio::test]
    async fn test_parallel_snapshots_do_not_clash() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        let manager = std::sync::Arc::new(WorkspaceCheckpoint::new(root).await.expect("new manager"));
        
        let mut handles = vec![];
        for i in 0..5 {
            let m = manager.clone();
            let file_path = root.join(format!("file_{}.txt", i));
            handles.push(tokio::spawn(async move {
                std::fs::write(&file_path, b"content").unwrap();
                m.snapshot().await
            }));
        }
        
        for handle in handles {
            handle.await.expect("task panicked").expect("snapshot failed");
        }
    }
}
