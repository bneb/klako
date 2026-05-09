
use tokio::process::{Child, Command};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

/// An Atomic reaper natively coupling Linux Process Groups with Rust Drop semantics.
/// Solves "Architecture Debt" vulnerability where UI cancellation tokens leak orphans
/// in underlying `bwrap` supervisor threads.
pub struct SandboxGuard {
    pgid: Option<i32>,
    child: Option<Child>,
}

impl SandboxGuard {
    /// Spawns the command in a new isolated process group.
    pub fn spawn_isolated(mut command: Command) -> std::io::Result<Self> {
        // Force the child to become the leader of a new process group natively before executing binary.
        command.process_group(0);

        let child = command.spawn()?;
        let pgid = child.id().map(|id| id as i32);

        Ok(Self {
            pgid,
            child: Some(child),
        })
    }

    #[must_use] 
    pub fn pgid(&self) -> Option<i32> {
        self.pgid
    }

    pub fn child_mut(&mut self) -> Option<&mut Child> {
        self.child.as_mut()
    }

    /// Exposes the inner wait method allowing full async buffers while maintaining Drop constraints.
    pub async fn wait_with_output(mut self) -> std::io::Result<std::process::Output> {
        let child = self.child.take().expect("Child already consumed");
        let result = child.wait_with_output().await;

        // Disarm the Drop Reaper natively if the invocation finished successfully 
        // without getting canceled by multi-token interrupts.
        self.pgid = None;

        result
    }
}

impl Drop for SandboxGuard {
    fn drop(&mut self) {
        if let Some(pgid) = self.pgid {
            // The minus sign is critical: it sends SIGKILL to the entire process group natively,
            // tearing down the `bwrap` supervisor and any wildly nested forks it synthesized.
            let _ = kill(Pid::from_raw(-pgid), Signal::SIGKILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    

    #[tokio::test]
    async fn test_sandbox_reaper_kills_descendants() {
        // We will spawn a shell that puts a long-running descendant in the background.
        // It creates a new process group.
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 1000 & sleep 1000");

        let guard = SandboxGuard::spawn_isolated(cmd).expect("failed to spawn");
        let pgid = guard.pgid.expect("no pgid");

        // Give it a moment to boot up safely
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify the process group natively exists by sending a 0 signal.
        let check_alive = kill(Pid::from_raw(-pgid), None);
        assert!(check_alive.is_ok(), "process group should be alive initially");

        // Force dropping the guard (simulating Tokio future drop on cancellation)
        drop(guard);

        // Wait minimal time for the kernel to distribute the SIGKILL
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify that the process group has been utterly obliterated natively.
        let check_dead = kill(Pid::from_raw(-pgid), None);
        assert!(check_dead.is_err(), "process group should be annihilated");
    }
}
