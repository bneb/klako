use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct WorkspaceCheckpoint {
    work_tree: PathBuf,
    git_dir: PathBuf,
    ignore_file: PathBuf,
}

impl WorkspaceCheckpoint {
    pub async fn new(workspace_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let work_tree = workspace_root.as_ref().to_path_buf();
        let klako_dir = work_tree.join(".klako");
        let git_dir = klako_dir.join("shadow.git");
        let ignore_file = klako_dir.join("shadow_ignore");

        let manager = Self {
            work_tree,
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
            .env("GIT_WORK_TREE", &self.work_tree)
            // Prevent global config and user aliases from interfering
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            // Apply the local isolation exclusions to avoid blowing out I/O
            .arg("-c")
            .arg(format!("core.excludesFile={}", self.ignore_file.display()));
        cmd
    }

    async fn init_if_needed(&self) -> std::io::Result<()> {
        let klako_dir = self.work_tree.join(".klako");
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
                .args(["init", "--bare", "--initial-branch=main"])
                .arg(&self.git_dir)
                .status()
                .await?;
                
            if !status.success() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to initialize shadow git repo",
                ));
            }
            
            // Perform an initial empty commit so there's always a valid HEAD
            let commit_status = self.git()
                .args(["commit", "--allow-empty", "-m", "Initial Bound"])
                .status()
                .await?;
                
            if !commit_status.success() {
                 return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to generate valid HEAD",
                ));
            }
        }
        Ok(())
    }

    /// Takes an instantaneous delta snapshot into the shadow tracker
    pub async fn snapshot(&self) -> std::io::Result<()> {
        let add_status = self.git().args(["add", "-A"]).status().await?;
        if !add_status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to index workspace during snapshot",
            ));
        }

        let _ = self
            .git()
            .args(["commit", "--allow-empty", "-m", "Auto-Checkpoint"])
            .status()
            .await?;

        Ok(())
    }

    /// Executes literal instantaneous Undo dropping the user's workspace back to the `snapshot()` tracker.
    pub async fn restore(&self) -> std::io::Result<()> {
        self.restore_commits(0).await
    }

    /// Restores the workspace by rewinding a specific number of snapshot commits.
    pub async fn restore_commits(&self, rewinds: usize) -> std::io::Result<()> {
        // Discard any untracked untamed trash generated
        let clean_status = self.git().args(["clean", "-df"]).status().await?;
        if !clean_status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to clean untracked files during restore",
            ));
        }

        let target = format!("HEAD~{}", rewinds);
        let reset_status = self.git().args(["reset", "--hard", &target]).status().await?;
        if !reset_status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to hard reset during restore",
            ));
        }

        Ok(())
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
}
