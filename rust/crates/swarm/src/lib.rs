use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use runtime::{ApiClient, ApiRequest, AssistantEvent, ConversationMessage, Session};
use runtime::workspace::checkpoint::WorkspaceCheckpoint;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwarmObjective {
    pub description: String,
    pub budget: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SwarmStatus {
    Idle,
    Planning,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmTaskStatus {
    Pending,
    Running,
    Verifying,
    VerifyingAxioms,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub description: String,
    pub status: SwarmTaskStatus,
    pub verification_tool: Option<String>,
    pub verification_input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmAgent {
    pub id: String,
    pub subagent_type: String,
    pub status: String,
    pub task_index: Option<usize>,
}

pub struct SwarmOrchestrator {
    session: Session,
    objective: SwarmObjective,
    status: SwarmStatus,
    tasks: Vec<SwarmTask>,
    agents: Vec<SwarmAgent>,
    provider: Box<dyn ApiClient>,
    checkpoint: Option<WorkspaceCheckpoint>,
    cancel_token: CancellationToken,
}

impl SwarmOrchestrator {
    pub async fn new(
        session: Session,
        objective: SwarmObjective,
        provider: Box<dyn ApiClient>,
    ) -> Self {
        let checkpoint = WorkspaceCheckpoint::new(".").await.ok();
        Self {
            session,
            objective,
            status: SwarmStatus::Idle,
            tasks: Vec::new(),
            agents: Vec::new(),
            provider,
            checkpoint,
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    #[must_use] 
    pub fn status(&self) -> SwarmStatus {
        self.status.clone()
    }

    #[must_use] 
    pub fn tasks(&self) -> &[SwarmTask] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut [SwarmTask] {
        &mut self.tasks
    }

    #[must_use] 
    pub fn agents(&self) -> &[SwarmAgent] {
        &self.agents
    }

    pub async fn start(&mut self) -> Result<(), String> {
        self.status = SwarmStatus::Planning;
        
        let request = ApiRequest {
            system_prompt: vec!["You are an expert project architect. Decompose the following objective into a set of atomic, verifiable engineering tasks. Return the tasks as a JSON list of objects with a 'description' field. Use only JSON, no other text.".to_string()],
            messages: vec![ConversationMessage::user_text(self.objective.description.clone())],
        };

        let events = self.provider.stream(request).await.map_err(|e| e.to_string())?;
        let mut full_text = String::new();
        for event in events {
            if let AssistantEvent::TextDelta(text) = event {
                full_text.push_str(&text);
            }
        }

        let json_start = full_text.find('[').ok_or("No JSON array found in architect response")?;
        let json_end = full_text.rfind(']').ok_or("No JSON array end found in architect response")?;
        let json_str = &full_text[json_start..=json_end];
        
        let tasks_data: Vec<serde_json::Value> = serde_json::from_str(json_str).map_err(|e| e.to_string())?;
        for task_val in tasks_data {
            if let Some(desc) = task_val.get("description").and_then(|v| v.as_str()) {
                self.tasks.push(SwarmTask {
                    description: desc.to_string(),
                    status: SwarmTaskStatus::Pending,
                    verification_tool: None,
                    verification_input: None,
                });
            }
        }

        let mut plan_content = format!("# Swarm Execution Plan\n\n**Objective:** {}\n\n", self.objective.description);
        for task in &self.tasks {
            plan_content.push_str(&format!("- {}\n", task.description));
        }
        
        std::fs::create_dir_all(".kla/sessions").map_err(|e| e.to_string())?;
        std::fs::write(".kla/sessions/PLAN.md", plan_content).map_err(|e| e.to_string())?;
        
        self.emit_ledger_update();
        Ok(())
    }

    pub async fn approve_plan(&mut self) -> Result<(), String> {
        if self.status != SwarmStatus::Planning {
            return Err("Cannot approve plan when not in Planning state".to_string());
        }
        
        // Re-parse PLAN.md in case user edited it
        let plan_md = std::fs::read_to_string(".kla/sessions/PLAN.md").map_err(|e| e.to_string())?;
        self.tasks.clear();
        for line in plan_md.lines() {
            if let Some(task_desc) = line.strip_prefix("- ") {
                self.tasks.push(SwarmTask {
                    description: task_desc.trim().to_string(),
                    status: SwarmTaskStatus::Pending,
                    verification_tool: None,
                    verification_input: None,
                });
            }
        }

        self.status = SwarmStatus::Running;
        self.emit_ledger_update();
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<(), String> {
        if self.status != SwarmStatus::Running {
            return Ok(());
        }

        // 0. Budget Check
        if let Some(budget_mtoks) = self.objective.budget {
            let total_tokens = self.calculate_total_usage();
            let total_mtoks = f64::from(total_tokens) / 1_000_000.0;
            if total_mtoks > budget_mtoks {
                let err = format!("Budget exceeded: {total_mtoks:.4}M / {budget_mtoks:.4}M tokens");
                self.status = SwarmStatus::Failed(err.clone());
                self.emit_ledger_update();
                return Err(err);
            }
        }

        // 1. Check if all tasks are done
        if !self.tasks.is_empty() && self.tasks.iter().all(|t| matches!(t.status, SwarmTaskStatus::Completed)) {
            self.status = SwarmStatus::Completed;
            self.emit_ledger_update();
            return Ok(());
        }

        // 2. Poll running subagents
        let mut ready_for_axiom_validation = Vec::new();
        let mut verifications = Vec::new();
        let mut snapshot_needed = false;

        for agent in &mut self.agents {
            if agent.status == "running" {
                if let Some(task_idx) = agent.task_index {
                    let manifest_path = PathBuf::from(format!(".kla-agents/{}.json", agent.id));
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(status) = manifest.get("status").and_then(|v| v.as_str()) {
                                if status == "completed" {
                                    agent.status = "verifying_axioms".to_string();
                                    ready_for_axiom_validation.push(task_idx);
                                } else if status == "failed" {
                                    agent.status = "failed".to_string();
                                    let err = manifest.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                                    self.tasks[task_idx].status = SwarmTaskStatus::Failed(err.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        for (index, task) in self.tasks.iter_mut().enumerate() {
            if task.status == SwarmTaskStatus::Verifying {
                if let Some(tool_name) = &task.verification_tool {
                    let fallback = serde_json::json!({});
                    let input = task.verification_input.as_ref().unwrap_or(&fallback);
                    match tools::execute_tool(tool_name, input).await {
                        Ok(_) => {
                            verifications.push((index, SwarmTaskStatus::VerifyingAxioms));
                            snapshot_needed = true;
                        }
                        Err(err) => {
                            verifications.push((index, SwarmTaskStatus::Failed(format!("Verification failed: {err}"))));
                        }
                    }
                } else {
                    verifications.push((index, SwarmTaskStatus::VerifyingAxioms));
                    snapshot_needed = true;
                }
            }
        }

        for (index, new_status) in verifications {
            self.tasks[index].status = new_status;
        }

        for idx in &ready_for_axiom_validation {
            self.tasks[*idx].status = SwarmTaskStatus::VerifyingAxioms;
        }

        if snapshot_needed || !ready_for_axiom_validation.is_empty() {
            if let Some(cp) = &self.checkpoint {
                let _ = cp.snapshot().await;
            }
            self.emit_ledger_update();
            return Ok(());
        }

        // 2b. Handle Axiom Validation
        let mut axiom_validated = Vec::new();
        for (idx, task) in self.tasks.iter_mut().enumerate() {
            if task.status == SwarmTaskStatus::VerifyingAxioms {
                let axioms = std::fs::read_to_string("KLA.md").unwrap_or_default();
                if axioms.is_empty() {
                    axiom_validated.push(idx);
                    continue;
                }

                let cp = self.checkpoint.as_ref().ok_or("No checkpoint found")?;
                let diff = cp.get_current_diff().await.unwrap_or_default();

                let mut validator = runtime::axiom::AxiomValidator::new(dyn_clone::clone_box(&*self.provider));
                match validator.validate(&axioms, &diff).await {
                    Ok(res) => {
                        if res.passed {
                            axiom_validated.push(idx);
                        } else {
                            task.status = SwarmTaskStatus::Failed(format!("Axiom Violation: {}", res.reasoning));
                        }
                    }
                    Err(e) => {
                        task.status = SwarmTaskStatus::Failed(format!("Validator Error: {e}"));
                    }
                }
            }
        }

        for idx in axiom_validated {
            self.tasks[idx].status = SwarmTaskStatus::Completed;
        }

        if !self.tasks.iter().any(|t| t.status == SwarmTaskStatus::VerifyingAxioms) {
            if let Some((index, task)) = self.tasks.iter_mut().enumerate().find(|(_, t)| t.status == SwarmTaskStatus::Pending) {
                task.status = SwarmTaskStatus::Running;
                let description = task.description.clone();
                
                let input = serde_json::json!({
                    "subagent_type": "Engineer",
                    "description": format!("Task: {}", description),
                    "prompt": format!("Please complete this task: {}", description)
                });
                
                let result_str = tools::execute_tool("Delegate", &input).await?;
                let output: serde_json::Value = serde_json::from_str(&result_str).map_err(|e| e.to_string())?;
                let agent_id = output.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");

                self.agents.push(SwarmAgent {
                    id: agent_id.to_string(),
                    subagent_type: "Engineer".to_string(),
                    status: "running".to_string(),
                    task_index: Some(index),
                });
            }
        }

        self.emit_ledger_update();
        Ok(())
    }

    pub async fn complete_task(&mut self, task_index: usize, _result: String) -> Result<(), String> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            if task.verification_tool.is_some() {
                task.status = SwarmTaskStatus::Verifying;
            } else {
                task.status = SwarmTaskStatus::VerifyingAxioms;
                if let Some(cp) = &self.checkpoint {
                    let _ = cp.snapshot().await;
                }
            }
            self.emit_ledger_update();
            Ok(())
        } else {
            Err("Task not found".to_string())
        }
    }

    pub async fn fail_task(&mut self, task_index: usize, error: String) -> Result<(), String> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.status = SwarmTaskStatus::Failed(error);
            self.emit_ledger_update();
        }
        Ok(())
    }

    fn emit_ledger_update(&self) {
        let payload = serde_json::json!({
            "type": "SwarmLedgerUpdate",
            "status": self.status,
            "objective": self.objective,
            "tasks": self.tasks,
            "agents": self.agents,
        });
        
        let _ = std::fs::create_dir_all(".kla/sessions");
        let _ = std::fs::write(".kla/sessions/SWARM_STATE.json", serde_json::to_string_pretty(&payload).unwrap_or_default());
        tools::emit_telemetry(payload);
    }

    fn calculate_total_usage(&self) -> u32 {
        let mut total = 0;
        
        // 1. Primary session usage
        total += runtime::UsageTracker::from_session(&self.session).cumulative_usage().total_tokens();

        // 2. Sub-agent session usage (from disk)
        if let Ok(entries) = std::fs::read_dir(".kla/sessions") {
            for entry in entries.filter_map(std::result::Result::ok) {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("session-agent-") && name.ends_with(".json") {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(session) = serde_json::from_str::<runtime::Session>(&content) {
                                total += runtime::UsageTracker::from_session(&session).cumulative_usage().total_tokens();
                            }
                        }
                    }
                }
            }
        }

        total
    }
}
