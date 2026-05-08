use runtime::{Session, MessageRole, ContentBlock, ConversationMessage, ApiClient, ApiRequest, AssistantEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmStatus {
    Idle,
    Planning,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmObjective {
    pub description: String,
    pub budget: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmTaskStatus {
    Pending,
    Running,
    Verifying,
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

use runtime::workspace::checkpoint::WorkspaceCheckpoint;

pub struct SwarmOrchestrator {
    session: Session,
    objective: SwarmObjective,
    status: SwarmStatus,
    tasks: Vec<SwarmTask>,
    agents: Vec<SwarmAgent>,
    provider: Box<dyn ApiClient>,
    checkpoint: Option<WorkspaceCheckpoint>,
}

impl SwarmOrchestrator {
    pub async fn new(session: Session, objective: SwarmObjective, provider: Box<dyn ApiClient>) -> Self {
        let checkpoint = WorkspaceCheckpoint::new(".").await.ok();
        Self {
            session,
            objective,
            status: SwarmStatus::Idle,
            tasks: Vec::new(),
            agents: Vec::new(),
            provider,
            checkpoint,
        }
    }

    pub fn status(&self) -> SwarmStatus {
        self.status.clone()
    }

    pub fn tasks(&self) -> &[SwarmTask] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut [SwarmTask] {
        &mut self.tasks
    }

    pub fn agents(&self) -> &[SwarmAgent] {
        &self.agents
    }

    pub async fn start(&mut self) -> Result<(), String> {
        self.status = SwarmStatus::Planning;
        self.emit_ledger_update();
        
        let planning_prompt = format!(
            "You are a Tier-1 Architect agent. Decompose the following objective into a set of atomic, verifiable engineering tasks: {}\n\nReturn the tasks as a JSON list of objects with a 'description' field. Use only JSON, no other text.",
            self.objective.description
        );

        let request = ApiRequest {
            system_prompt: vec!["You are an expert software architect.".to_string()],
            messages: vec![ConversationMessage {
                role: MessageRole::User,
                blocks: vec![ContentBlock::Text { text: planning_prompt }],
                usage: None,
            }],
        };

        // For simplicity in lib, we use a non-streaming call if possible, 
        // but since our ApiClient only has stream, we use it.
        let events = self.provider.stream(request).map_err(|e| e.to_string())?;
        let mut full_text = String::new();
        for event in events {
            if let AssistantEvent::TextDelta(text) = event {
                full_text.push_str(&text);
            }
        }

        // Simple JSON extraction
        if let Some(json_start) = full_text.find('[') {
            if let Some(json_end) = full_text.rfind(']') {
                let json_str = &full_text[json_start..=json_end];
                if let Ok(tasks_data) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
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
                }
            }
        }

        if self.tasks.is_empty() {
            self.tasks.push(SwarmTask {
                description: format!("Analyze: {}", self.objective.description),
                status: SwarmTaskStatus::Pending,
                verification_tool: None,
                verification_input: None,
            });
        }
        
        // Write the plan to PLAN.md
        std::fs::create_dir_all(".kla/sessions").ok();
        let mut plan_content = format!("# Swarm Execution Plan\n\n**Objective:** {}\n\n*Edit the tasks below. Each line starting with `- ` is a task. Save the file and approve in the UI/CLI to begin execution.*\n\n", self.objective.description);
        for task in &self.tasks {
            plan_content.push_str(&format!("- {}\n", task.description));
        }
        std::fs::write(".kla/sessions/PLAN.md", plan_content).map_err(|e| e.to_string())?;
        
        self.emit_ledger_update();
        Ok(())
    }

    pub async fn approve_plan(&mut self) -> Result<(), String> {
        self.status = SwarmStatus::Running;
        
        // Read PLAN.md and rebuild tasks
        if let Ok(content) = std::fs::read_to_string(".kla/sessions/PLAN.md") {
            let mut new_tasks = Vec::new();
            for line in content.lines() {
                if let Some(desc) = line.strip_prefix("- ") {
                    new_tasks.push(SwarmTask {
                        description: desc.trim().to_string(),
                        status: SwarmTaskStatus::Pending,
                        verification_tool: None,
                        verification_input: None,
                    });
                }
            }
            if !new_tasks.is_empty() {
                self.tasks = new_tasks;
            }
        }
        
        self.emit_ledger_update();
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<(), String> {
        if self.tasks.iter().all(|t| t.status == SwarmTaskStatus::Completed) && !self.tasks.is_empty() {
            self.status = SwarmStatus::Completed;
            self.emit_ledger_update();
            return Ok(());
        }

        // Budget check (in Millions of Tokens)
        if let Some(budget_m_tokens) = self.objective.budget {
            let mut total_tokens = 0;
            for agent in &self.agents {
                let session_path = std::path::PathBuf::from(format!(".kla/sessions/session-{}.json", agent.id));
                if let Ok(session) = runtime::Session::load_from_path(&session_path) {
                    let tracker = runtime::UsageTracker::from_session(&session);
                    total_tokens += tracker.cumulative_usage().total_tokens();
                }
            }
            let total_m_tokens = (total_tokens as f64) / 1_000_000.0;
            if total_m_tokens > budget_m_tokens {
                self.status = SwarmStatus::Failed(format!("Token budget exceeded: {:.2}M / {:.2}M tokens", total_m_tokens, budget_m_tokens));
                self.emit_ledger_update();
                return Ok(());
            }
        }

        // 1. Check for tasks in Verifying state to run deterministic tools
        let mut verifications = Vec::new();
        let mut snapshot_needed = false;
        for (index, task) in self.tasks.iter().enumerate() {
            if task.status == SwarmTaskStatus::Verifying {
                if let Some(tool_name) = &task.verification_tool {
                    let fallback = serde_json::json!({});
                    let input = task.verification_input.as_ref().unwrap_or(&fallback);
                    match tools::execute_tool(tool_name, input) {
                        Ok(_) => {
                            verifications.push((index, SwarmTaskStatus::Completed));
                            snapshot_needed = true;
                        }
                        Err(err) => {
                            verifications.push((index, SwarmTaskStatus::Failed(format!("Verification failed: {}", err))));
                        }
                    }
                } else {
                    verifications.push((index, SwarmTaskStatus::Completed));
                    snapshot_needed = true;
                }
            }
        }

        for (index, new_status) in verifications {
            if let Some(task) = self.tasks.get_mut(index) {
                task.status = new_status;
            }
        }

        if snapshot_needed {
            if let Some(cp) = &self.checkpoint {
                let _ = cp.snapshot().await;
            }
        }

        // 2. Check for Pending tasks to spawn agents
        if let Some((index, task)) = self.tasks.iter_mut().enumerate().find(|(_, t)| t.status == SwarmTaskStatus::Pending) {
            task.status = SwarmTaskStatus::Running;
            let description = task.description.clone();
            self.emit_ledger_update();
            
            let input = serde_json::json!({
                "subagent_type": "Engineer",
                "description": format!("Task: {}", description),
                "prompt": format!("Please complete this task: {}", description)
            });
            
            let result_str = tools::execute_tool("Delegate", &input)?;
            let output: serde_json::Value = serde_json::from_str(&result_str).map_err(|e| e.to_string())?;
            let agent_id = output.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");

            self.agents.push(SwarmAgent {
                id: agent_id.to_string(),
                subagent_type: "Engineer".to_string(),
                status: "running".to_string(),
                task_index: Some(index),
            });
            self.emit_ledger_update();
        }

        if !self.tasks.is_empty() && self.status == SwarmStatus::Running {
            self.emit_ledger_update();
        }

        Ok(())
    }

    pub async fn complete_task(&mut self, task_index: usize, _result: String) -> Result<(), String> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            if task.verification_tool.is_some() {
                task.status = SwarmTaskStatus::Verifying;
            } else {
                task.status = SwarmTaskStatus::Completed;
                if let Some(cp) = &self.checkpoint {
                    let _ = cp.snapshot().await;
                }
            }
            self.emit_ledger_update();
        }
        Ok(())
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
        
        // Serialize state to disk for the /rewind feature
        let _ = std::fs::create_dir_all(".kla/sessions");
        let _ = std::fs::write(".kla/sessions/SWARM_STATE.json", serde_json::to_string_pretty(&payload).unwrap_or_default());

        tools::agent::emit_telemetry(payload);
    }
}
