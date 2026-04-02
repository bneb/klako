use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

/// Run a git command and return its stdout as a String.
///
/// Fails if the command exits non-zero.
pub fn git_output(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Run a git command and assert success, discarding stdout.
pub fn git_status_ok(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(())
}

/// Resolve the absolute path of the git repository root for the current
/// working directory.
pub fn find_git_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        return Err("not a git repository".into());
    }
    let path = String::from_utf8(output.stdout)?.trim().to_string();
    if path.is_empty() {
        return Err("empty git root".into());
    }
    Ok(PathBuf::from(path))
}

/// Parse `git status --short -b` output into (project_root, branch).
///
/// Returns `(None, None)` when `status` is `None`.
pub fn parse_git_status_metadata(status: Option<&str>) -> (Option<PathBuf>, Option<String>) {
    let Some(status) = status else {
        return (None, None);
    };
    let branch = status.lines().next().and_then(|line| {
        line.strip_prefix("## ")
            .map(|line| {
                line.split(['.', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    });
    let project_root = find_git_root().ok();
    (project_root, branch)
}

/// Return `true` if `name` is an executable on `$PATH`.
pub fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Write `contents` to `$TMPDIR/<filename>` and return the path.
pub fn write_temp_text_file(
    filename: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = env::temp_dir().join(filename);
    fs::write(&path, contents)?;
    Ok(path)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_git_status_metadata_returns_none_for_none_input() {
        let (root, branch) = parse_git_status_metadata(None);
        assert!(root.is_none());
        assert!(branch.is_none());
    }

    #[test]
    fn parse_git_status_metadata_extracts_branch_from_status_line() {
        // Simulated `git status --short -b` header line.
        let status = "## main...origin/main";
        let (_, branch) = parse_git_status_metadata(Some(status));
        assert_eq!(branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_git_status_metadata_handles_detached_head() {
        let status = "## HEAD (no branch)";
        let (_, branch) = parse_git_status_metadata(Some(status));
        assert_eq!(branch.as_deref(), Some("HEAD"));
    }

    #[test]
    fn command_exists_returns_false_for_nonexistent_binary() {
        assert!(!command_exists("__no_such_binary_klako_test__"));
    }

    #[test]
    fn write_temp_text_file_round_trips_content() {
        let filename = "klako_git_test_tmp.txt";
        let contents = "hello from kla";
        let path = write_temp_text_file(filename, contents).expect("write succeeds");
        let read_back = std::fs::read_to_string(&path).expect("read succeeds");
        assert_eq!(read_back, contents);
        let _ = std::fs::remove_file(path);
    }
}
