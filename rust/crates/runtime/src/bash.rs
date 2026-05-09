use std::env;
use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

use crate::sandbox::{
    build_linux_sandbox_command, resolve_sandbox_status_for_request, FilesystemIsolationMode,
    SandboxConfig, SandboxStatus, reaper::SandboxGuard,
};
use crate::ConfigLoader;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use std::time::{SystemTime, UNIX_EPOCH};

static PERSISTENT_SHELL: OnceLock<TokioMutex<Option<PersistentShell>>> = OnceLock::new();

fn persistent_shell() -> &'static TokioMutex<Option<PersistentShell>> {
    PERSISTENT_SHELL.get_or_init(|| TokioMutex::new(None))
}

static DELIMITER_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_delimiter() -> String {
    let micros = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros();
    let count = DELIMITER_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__KLAKO_DELIM_{micros}_{count}__")
}

async fn read_until_delimiter(
    stream: &mut (impl tokio::io::AsyncRead + std::marker::Unpin),
    delimiter: &str,
) -> std::io::Result<(String, String)> {
    let mut buf = vec![0; 1024];
    let mut captured_bytes = Vec::new();
    let delim_bytes = delimiter.as_bytes();
    
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            // EOF reached without delimiter
            let out = String::from_utf8_lossy(&captured_bytes).into_owned();
            return Ok((out, String::new()));
        }
        captured_bytes.extend_from_slice(&buf[..n]);
        
        let cap_len = captured_bytes.len();
        let delim_len = delim_bytes.len();
        
        if cap_len >= delim_len {
            // Check if delimiter exists in the captured bytes so far
            if let Some(pos) = captured_bytes.windows(delim_len).position(|w| w == delim_bytes) {
                // Determine boundaries
                let col_idx = pos + delim_len;
                if col_idx < cap_len && captured_bytes[col_idx] == b':' {
                    let start = col_idx + 1;
                    if let Some(nl_offset) = captured_bytes[start..].iter().position(|&b| b == b'\n') {
                        let nl = start + nl_offset;
                        let code_bytes = &captured_bytes[start..nl];
                        let code_str = String::from_utf8_lossy(code_bytes).trim().to_string();
                        
                        // Output is everything before the delimiter
                        let mut out = String::from_utf8_lossy(&captured_bytes[..pos]).into_owned();
                        if out.ends_with('\n') {
                            out.pop();
                        }
                        
                        return Ok((out, code_str));
                    }
                }
            }
        }
    }
}

pub struct PersistentShell {
    _guard: SandboxGuard,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
}

impl PersistentShell {
    pub fn spawn() -> std::io::Result<Self> {
        let mut command = tokio::process::Command::new("bash");
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut guard = SandboxGuard::spawn_isolated(command)?;
        let child = guard.child_mut().ok_or_else(|| io::Error::other("Child must exist after spawn"))?;
        
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("Failed to take stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("Failed to take stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| io::Error::other("Failed to take stderr"))?;

        Ok(Self { _guard: guard, stdin, stdout, stderr })
    }

    pub async fn execute(&mut self, command: &str, timeout_ms: Option<u64>) -> std::io::Result<(String, String, Option<i32>)> {
        let out_del = next_delimiter();
        let err_del = next_delimiter();
        let payload = format!(
            "{{ {command} ; }} \n_K_ST=$?\necho \"{out_del}:$_K_ST\"\necho \"{err_del}:$_K_ST\" >&2\n"
        );

        self.stdin.write_all(payload.as_bytes()).await?;
        self.stdin.flush().await?;

        let stdout_fut = read_until_delimiter(&mut self.stdout, &out_del);
        let stderr_fut = read_until_delimiter(&mut self.stderr, &err_del);

        let result = if let Some(t) = timeout_ms {
            match tokio::time::timeout(std::time::Duration::from_millis(t), async { tokio::join!(stdout_fut, stderr_fut) }).await {
                Ok(res) => res,
                Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Command timed out"))
            }
        } else {
            tokio::join!(stdout_fut, stderr_fut)
        };

        let (out_res, err_res) = result;
        let (stdout, code_str) = out_res?;
        let (stderr, _) = err_res?;

        let code = if code_str.is_empty() {
            // EOF hit. Check if shell exited
            match self._guard.child_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => status.code().or(Some(-1)),
                    _ => None,
                },
                None => None,
            }
        } else {
            code_str.parse::<i32>().ok()
        };
        
        Ok((stdout, stderr, code))
    }
}

use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct BashCommandInput {
    pub command: String,
    pub timeout: Option<u64>,
    pub description: Option<String>,
    #[serde(rename = "run_in_background")]
    pub run_in_background: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "namespaceRestrictions")]
    pub namespace_restrictions: Option<bool>,
    #[serde(rename = "isolateNetwork")]
    pub isolate_network: Option<bool>,
    #[serde(rename = "filesystemMode")]
    pub filesystem_mode: Option<FilesystemIsolationMode>,
    #[serde(rename = "allowedMounts")]
    pub allowed_mounts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BashCommandOutput {
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "rawOutputPath")]
    pub raw_output_path: Option<String>,
    pub interrupted: bool,
    #[serde(rename = "isImage")]
    pub is_image: Option<bool>,
    #[serde(rename = "backgroundTaskId")]
    pub background_task_id: Option<String>,
    #[serde(rename = "backgroundedByUser")]
    pub backgrounded_by_user: Option<bool>,
    #[serde(rename = "assistantAutoBackgrounded")]
    pub assistant_auto_backgrounded: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "returnCodeInterpretation")]
    pub return_code_interpretation: Option<String>,
    #[serde(rename = "noOutputExpected")]
    pub no_output_expected: Option<bool>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Option<Vec<serde_json::Value>>,
    #[serde(rename = "persistedOutputPath")]
    pub persisted_output_path: Option<String>,
    #[serde(rename = "persistedOutputSize")]
    pub persisted_output_size: Option<u64>,
    #[serde(rename = "sandboxStatus")]
    pub sandbox_status: Option<SandboxStatus>,
}

pub async fn execute_bash(input: BashCommandInput) -> io::Result<BashCommandOutput> {
    let cwd = env::current_dir()?;
    let sandbox_status = sandbox_status_for_input(&input, &cwd);

    if input.run_in_background.unwrap_or(false) {
        let mut child = prepare_command(&input.command, &cwd, &sandbox_status, false);
        let child = child
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        return Ok(BashCommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            raw_output_path: None,
            interrupted: false,
            is_image: None,
            background_task_id: Some(child.id().to_string()),
            backgrounded_by_user: Some(false),
            assistant_auto_backgrounded: Some(false),
            dangerously_disable_sandbox: input.dangerously_disable_sandbox,
            return_code_interpretation: None,
            no_output_expected: Some(true),
            structured_content: None,
            persisted_output_path: None,
            persisted_output_size: None,
            sandbox_status: Some(sandbox_status),
        });
    }

    if sandbox_status.filesystem_active {
        execute_bash_async(input, sandbox_status, cwd).await
    } else {
        let mut guard = persistent_shell().lock().await;
        if guard.is_none() {
            *guard = Some(PersistentShell::spawn()?);
        }
        let shell = guard.as_mut().unwrap();
        
        match shell.execute(&input.command, input.timeout).await {
            Ok((stdout, stderr, code)) => {
                let return_code_interpretation = code.and_then(|c| if c == 0 { None } else { Some(format!("exit_code:{c}")) });
                
                // Basic heuristic to attach pwd system note if command resembles directory change
                let no_output_expected = Some(stdout.trim().is_empty() && stderr.trim().is_empty());
                
                Ok(BashCommandOutput {
                    stdout,
                    stderr,
                    raw_output_path: None,
                    interrupted: false,
                    is_image: None,
                    background_task_id: None,
                    backgrounded_by_user: None,
                    assistant_auto_backgrounded: None,
                    dangerously_disable_sandbox: input.dangerously_disable_sandbox,
                    return_code_interpretation,
                    no_output_expected,
                    structured_content: None,
                    persisted_output_path: None,
                    persisted_output_size: None,
                    sandbox_status: Some(sandbox_status),
                })
            }
            Err(e) => {
                // If timeout or failure, shell might be poisoned. Nuke it.
                *guard = None;
                if e.kind() == io::ErrorKind::TimedOut {
                    Ok(BashCommandOutput {
                        stdout: String::new(),
                        stderr: format!("Command exceeded timeout of {} ms", input.timeout.unwrap_or(0)),
                        raw_output_path: None,
                        interrupted: true,
                        is_image: None,
                        background_task_id: None,
                        backgrounded_by_user: None,
                        assistant_auto_backgrounded: None,
                        dangerously_disable_sandbox: input.dangerously_disable_sandbox,
                        return_code_interpretation: Some("timeout".to_string()),
                        no_output_expected: Some(true),
                        structured_content: None,
                        persisted_output_path: None,
                        persisted_output_size: None,
                        sandbox_status: Some(sandbox_status),
                    })
                } else {
                    Err(e)
                }
            }
        }
    }
}

async fn execute_bash_async(
    input: BashCommandInput,
    sandbox_status: SandboxStatus,
    cwd: std::path::PathBuf,
) -> io::Result<BashCommandOutput> {
    let mut command = prepare_tokio_command(&input.command, &cwd, &sandbox_status, true);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let guard = SandboxGuard::spawn_isolated(command)?;

    let output_result = if let Some(timeout_ms) = input.timeout {
        match timeout(Duration::from_millis(timeout_ms), guard.wait_with_output()).await {
            Ok(result) => (result?, false),
            Err(_) => {
                return Ok(BashCommandOutput {
                    stdout: String::new(),
                    stderr: format!("Command exceeded timeout of {timeout_ms} ms"),
                    raw_output_path: None,
                    interrupted: true,
                    is_image: None,
                    background_task_id: None,
                    backgrounded_by_user: None,
                    assistant_auto_backgrounded: None,
                    dangerously_disable_sandbox: input.dangerously_disable_sandbox,
                    return_code_interpretation: Some(String::from("timeout")),
                    no_output_expected: Some(true),
                    structured_content: None,
                    persisted_output_path: None,
                    persisted_output_size: None,
                    sandbox_status: Some(sandbox_status),
                });
            }
        }
    } else {
        (guard.wait_with_output().await?, false)
    };

    let (output, interrupted) = output_result;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let no_output_expected = Some(stdout.trim().is_empty() && stderr.trim().is_empty());
    let return_code_interpretation = output.status.code().and_then(|code| {
        if code == 0 {
            None
        } else {
            Some(format!("exit_code:{code}"))
        }
    });

    Ok(BashCommandOutput {
        stdout,
        stderr,
        raw_output_path: None,
        interrupted,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox: input.dangerously_disable_sandbox,
        return_code_interpretation,
        no_output_expected,
        structured_content: None,
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status: Some(sandbox_status),
    })
}

fn sandbox_status_for_input(input: &BashCommandInput, cwd: &std::path::Path) -> SandboxStatus {
    let config = ConfigLoader::default_for(cwd).load().map_or_else(
        |_| SandboxConfig::default(),
        |runtime_config| runtime_config.sandbox().clone(),
    );
    let request = config.resolve_request(
        input.dangerously_disable_sandbox.map(|disabled| !disabled),
        input.namespace_restrictions,
        input.isolate_network,
        input.filesystem_mode,
        input.allowed_mounts.clone(),
    );
    resolve_sandbox_status_for_request(&request, cwd)
}

fn prepare_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> Command {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = Command::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        prepared.envs(launcher.env);
        return prepared;
    }

    let mut prepared = Command::new("sh");
    prepared.arg("-lc").arg(command).current_dir(cwd);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
    }
    prepared
}

fn prepare_tokio_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> TokioCommand {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = TokioCommand::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        prepared.envs(launcher.env);
        return prepared;
    }

    let mut prepared = TokioCommand::new("sh");
    prepared.arg("-lc").arg(command).current_dir(cwd);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
    }
    prepared
}

fn prepare_sandbox_dirs(cwd: &std::path::Path) {
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-home"));
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-tmp"));
}

#[cfg(test)]
mod tests {
    use super::{execute_bash, BashCommandInput};
    use crate::sandbox::FilesystemIsolationMode;

    #[tokio::test]
    async fn executes_simple_command() {
        let output = execute_bash(BashCommandInput {
            command: String::from("printf 'hello'"),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(false),
            namespace_restrictions: Some(false),
            isolate_network: Some(false),
            filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
            allowed_mounts: None,
        })
        .await
        .expect("bash command should execute");

        assert_eq!(output.stdout, "hello");
        assert!(!output.interrupted);
        assert!(output.sandbox_status.is_some());
    }

    #[tokio::test]
    async fn disables_sandbox_when_requested() {
        let output = execute_bash(BashCommandInput {
            command: String::from("printf 'hello'"),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(true),
            namespace_restrictions: None,
            isolate_network: None,
            filesystem_mode: None,
            allowed_mounts: None,
        })
        .await
        .expect("bash command should execute");

        assert!(!output.sandbox_status.expect("sandbox status").enabled);
    }
}
