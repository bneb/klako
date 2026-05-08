use runtime::{Session, MessageRole, ContentBlock, ConversationMessage, ApiClient, ApiRequest, AssistantEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmStatus {
    Idle,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmObjective {
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmTaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub description: String,
    pub status: SwarmTaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmAgent {
    pub id: String,
    pub subagent_type: String,
    pub status: String,
}

pub struct SwarmOrchestrator {
    session: Session,
    objective: SwarmObjective,
    status: SwarmStatus,
    tasks: Vec<SwarmTask>,
    agents: Vec<SwarmAgent>,
    provider: Box<dyn ApiClient>,
}

impl SwarmOrchestrator {
    pub fn new(session: Session, objective: SwarmObjective, provider: Box<dyn ApiClient>) -> Self {
        Self {
            session,
            objective,
            status: SwarmStatus::Idle,
            tasks: Vec::new(),
            agents: Vec::new(),
            provider,
        }
    }

    pub fn status(&self) -> SwarmStatus {
        self.status.clone()
    }

    pub fn tasks(&self) -> &[SwarmTask] {
        &self.tasks
    }

    pub fn agents(&self) -> &[SwarmAgent] {
        &self.agents
    }

    pub async fn start(&mut self) -> Result<(), String> {
        self.status = SwarmStatus::Running;
        
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
            });
        }
        
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<(), String> {
        if self.tasks.iter().all(|t| t.status == SwarmTaskStatus::Completed) && !self.tasks.is_empty() {
            self.status = SwarmStatus::Completed;
            return Ok(());
        }

        if let Some(task) = self.tasks.iter_mut().find(|t| t.status == SwarmTaskStatus::Pending) {
            task.status = SwarmTaskStatus::Running;
            
            let input = serde_json::json!({
                "subagent_type": "Engineer",
                "description": format!("Task: {}", task.description),
                "prompt": format!("Please complete this task: {}", task.description)
            });
            
            let result_str = tools::execute_tool("Delegate", &input)?;
            let output: serde_json::Value = serde_json::from_str(&result_str).map_err(|e| e.to_string())?;
            let agent_id = output.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown");

            self.agents.push(SwarmAgent {
                id: agent_id.to_string(),
                subagent_type: "Engineer".to_string(),
                status: "running".to_string(),
            });
        }
        Ok(())
    }

    pub async fn complete_task(&mut self, task_index: usize, _result: String) -> Result<(), String> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.status = SwarmTaskStatus::Completed;
        }
        Ok(())
    }

    pub async fn fail_task(&mut self, task_index: usize, error: String) -> Result<(), String> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.status = SwarmTaskStatus::Failed(error);
        }
        Ok(())
    }
}
