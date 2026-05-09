use std::io::{self, Write, IsTerminal};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use crate::INTERNAL_PROGRESS_HEARTBEAT_INTERVAL;

#[derive(Debug, Clone)]
pub struct InternalPromptProgressReporter {
    shared: Arc<InternalPromptProgressShared>,
}

#[derive(Debug)]
pub struct InternalPromptProgressRun {
    reporter: InternalPromptProgressReporter,
    heartbeat_stop: Option<mpsc::Sender<()>>,
    heartbeat_handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InternalPromptProgressState {
    pub(crate) command_label: &'static str,
    pub(crate) task_label: String,
    pub(crate) step: usize,
    pub(crate) phase: String,
    pub(crate) detail: Option<String>,
    pub(crate) saw_final_text: bool,
}

#[derive(Debug)]
struct InternalPromptProgressShared {
    state: Mutex<InternalPromptProgressState>,
    output_lock: Mutex<()>,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalPromptProgressEvent {
    Started,
    Update,
    Heartbeat,
    Complete,
    Failed,
}

impl InternalPromptProgressReporter {
    #[must_use] 
    pub fn ultraplan(task: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Ultraplan",
                    task_label: task.to_string(),
                    step: 0,
                    phase: "planning started".to_string(),
                    detail: Some(format!("task: {task}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn bughunter(scope: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Bug Hunter",
                    task_label: scope.to_string(),
                    step: 0,
                    phase: "hunting started".to_string(),
                    detail: Some(format!("scope: {scope}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn design(feature: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Architect",
                    task_label: feature.to_string(),
                    step: 0,
                    phase: "design started".to_string(),
                    detail: Some(format!("feature: {feature}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn pr(context: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Staff Writer",
                    task_label: context.to_string(),
                    step: 0,
                    phase: "PR drafting started".to_string(),
                    detail: Some(format!("context: {context}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn issue(context: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "QA Engineer",
                    task_label: context.to_string(),
                    step: 0,
                    phase: "issue drafting started".to_string(),
                    detail: Some(format!("context: {context}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn commit() -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Commit Assist",
                    task_label: "analyzing changes".to_string(),
                    step: 0,
                    phase: "analyzing started".to_string(),
                    detail: None,
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    #[must_use] 
    pub fn start(&self) -> InternalPromptProgressRun {
        let (tx, rx) = mpsc::channel();
        let reporter = self.clone();

        self.emit(InternalPromptProgressEvent::Started);

        let heartbeat_handle = thread::spawn(move || loop {
            match rx.recv_timeout(INTERNAL_PROGRESS_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    reporter.emit(InternalPromptProgressEvent::Heartbeat);
                }
            }
        });

        InternalPromptProgressRun {
            reporter: self.clone(),
            heartbeat_stop: Some(tx),
            heartbeat_handle: Some(heartbeat_handle),
        }
    }

    pub fn mark_model_phase(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.step += 1;
        state.phase = "model reasoning".to_string();
        state.detail = None;
        drop(state);
        self.emit(InternalPromptProgressEvent::Update);
    }

    pub fn mark_final_text(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.saw_final_text = true;
    }

    fn emit(&self, event: InternalPromptProgressEvent) {
        let state = self.shared.state.lock().unwrap();
        let _lock = self.shared.output_lock.lock().unwrap();

        if state.saw_final_text && event != InternalPromptProgressEvent::Complete {
            return;
        }

        if !io::stdout().is_terminal() {
            return;
        }

        match event {
            InternalPromptProgressEvent::Started => {
                println!(
                    " \x1b[38;5;238m•\x1b[0m \x1b[1;38;5;45m{}\x1b[0m \x1b[2m· {}\x1b[0m",
                    state.command_label, state.task_label
                );
            }
            InternalPromptProgressEvent::Update | InternalPromptProgressEvent::Heartbeat => {
                let elapsed = self.shared.started_at.elapsed().as_secs();
                let dots = match elapsed % 4 {
                    0 => ".   ",
                    1 => "..  ",
                    2 => "... ",
                    _ => "....",
                };
                print!(
                    "\r \x1b[38;5;238m•\x1b[0m \x1b[1;38;5;45m{}\x1b[0m \x1b[2m· {} ({}{})\x1b[0m\x1b[K",
                    state.command_label,
                    state.phase,
                    elapsed,
                    dots
                );
                let _ = io::stdout().flush();
            }
            InternalPromptProgressEvent::Complete => {
                println!(
                    "\r \x1b[38;5;238m•\x1b[0m \x1b[1;38;5;45m{}\x1b[0m \x1b[2m· complete ({:?})\x1b[0m\x1b[K",
                    state.command_label,
                    self.shared.started_at.elapsed()
                );
            }
            InternalPromptProgressEvent::Failed => {
                println!(
                    "\r \x1b[38;5;238m•\x1b[0m \x1b[1;38;5;160m{}\x1b[0m \x1b[2m· failed ({:?})\x1b[0m\x1b[K",
                    state.command_label,
                    self.shared.started_at.elapsed()
                );
            }
        }
    }
}

impl Drop for InternalPromptProgressRun {
    fn drop(&mut self) {
        if let Some(tx) = self.heartbeat_stop.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.heartbeat_handle.take() {
            let _ = handle.join();
        }
        self.reporter.emit(InternalPromptProgressEvent::Complete);
    }
}
