use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

pub struct GitWorktree {
    pub absolute_path: PathBuf,
    pub branch_name: String,
}

impl GitWorktree {
    pub async fn spawn(workspace_root: &Path, task_id: &str) -> std::io::Result<Self> {
        let branch_name = format!("klako-sandbox/{task_id}");
        let absolute_path = workspace_root.join(".kla").join("sandbox").join(task_id);

        fs::create_dir_all(&absolute_path)?;
        
        let output = Command::new("git")
            .current_dir(workspace_root)
            .args(["rev-parse", "HEAD"])
            .output()?;
            
        let head = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let _ = Command::new("git")
            .current_dir(workspace_root)
            .args(["branch", &branch_name, &head])
            .output()?;

        let output = Command::new("git")
            .current_dir(workspace_root)
            .args(["worktree", "add", absolute_path.to_str().unwrap(), &branch_name])
            .output()?;

        if !output.status.success() {
            return Err(std::io::Error::other(
                format!("git worktree add failed: {}", String::from_utf8_lossy(&output.stderr)),
            ));
        }

        Ok(Self {
            absolute_path,
            branch_name,
        })
    }

    pub async fn teardown(self, workspace_root: &Path) -> std::io::Result<()> {
        let _ = Command::new("git")
            .current_dir(workspace_root)
            .args(["worktree", "remove", "--force", self.absolute_path.to_str().unwrap()])
            .output()?;
            
        let _ = Command::new("git")
            .current_dir(workspace_root)
            .args(["branch", "-D", &self.branch_name])
            .output()?;

        if self.absolute_path.exists() {
            let _ = fs::remove_dir_all(&self.absolute_path);
        }

        Ok(())
    }
}
