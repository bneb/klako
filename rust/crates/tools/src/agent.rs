use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{OnceLock, Mutex};

use api::{
    max_tokens_for_model, resolve_model_alias, ContentBlockDelta,
    MessageRequest, OutputContentBlock, ProviderClient,
    StreamEvent as ApiStreamEvent, ToolChoice, ToolDefinition,
};
use runtime::{
    load_system_prompt, ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage,
    ConversationRuntime, PermissionMode, RuntimeError, Session,
    TokenUsage, ToolError, ToolExecutor,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::{execute_tool, mvp_tool_specs, ToolSpec};

// ── Telemetry sink ───────────────────────────────────────────────────

static TELEMETRY_SINK: OnceLock<broadcast::Sender<String>> = OnceLock::new();

pub fn set_telemetry_sink(tx: broadcast::Sender<String>) {
    let _ = TELEMETRY_SINK.set(tx);
}

pub fn emit_telemetry(event: serde_json::Value) {
    if let Some(tx) = TELEMETRY_SINK.get() {
        let _ = tx.send(event.to_string());
    }
}

// ── Agent Registry ───────────────────────────────────────────────────

static AGENT_REGISTRY: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn agent_registry() -> &'static Mutex<HashMap<String, CancellationToken>> {
    AGENT_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cancel_all_agents() {
    let mut registry = agent_registry().lock().unwrap();
    for (_, token) in registry.drain() {
        token.cancel();
    }
    println!("Signal sent to all active sub-agents to terminate.");
}

// ── Constants ────────────────────────────────────────────────────────

const DEFAULT_AGENT_MODEL: &str = "gemini-2.5-flash";
const DEFAULT_AGENT_SYSTEM_DATE: &str = "2026-03-31";
const DEFAULT_AGENT_MAX_ITERATIONS: usize = 32;

use schemars::JsonSchema;

// ── Input types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AgentInput {
    pub description: String,
    pub prompt: String,
    pub subagent_type: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
}

// ── Output types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentOutput {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    #[serde(rename = "outputFile")]
    pub output_file: String,
    #[serde(rename = "manifestFile")]
    pub manifest_file: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub error: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentJob {
    pub manifest: AgentOutput,
    pub prompt: String,
    pub system_prompt: Vec<String>,
    pub allowed_tools: BTreeSet<String>,
    pub tx: Option<broadcast::Sender<String>>,
}

// ── Execution ────────────────────────────────────────────────────────

pub(crate) fn execute_agent(input: AgentInput) -> Result<AgentOutput, String> {
    execute_agent_with_spawn(input, spawn_agent_job)
}

pub(crate) fn execute_agent_with_spawn<F>(
    input: AgentInput,
    spawn_fn: F,
) -> Result<AgentOutput, String>
where
    F: FnOnce(AgentJob) -> Result<(), String>,
{
    let agent_id = make_agent_id();
    let agent_name = input.name.clone().unwrap_or_else(|| agent_id.clone());
    let normalized_subagent_type = input
        .subagent_type
        .clone().map_or_else(|| "Engineer".to_string(), |t| normalize_agent_type(&t));
    let allowed_tools = allowed_tools_for_type(&normalized_subagent_type, input.allowed_tools);
    let created_at = chrono::Utc::now().to_rfc3339();

    let store_dir = agent_store_dir()?;
    let output_file = store_dir
        .join(format!("{agent_id}.out"))
        .to_string_lossy()
        .to_string();
    let manifest_file = store_dir
        .join(format!("{agent_id}.json"))
        .to_string_lossy()
        .to_string();

    let manifest = AgentOutput {
        agent_id: agent_id.clone(),
        name: agent_name.clone(),
        description: input.description.clone(),
        status: "running".to_string(),
        output_file,
        manifest_file,
        created_at: created_at.clone(),
        error: None,
        model: input.model.clone(),
    };

    write_agent_manifest(&manifest)?;

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let system_prompt = load_system_prompt(&cwd, DEFAULT_AGENT_SYSTEM_DATE.to_string(), "linux", "6.8", &[])
        .map_err(|error| error.to_string())?;

    let output_contents = format!(
        "# Sub-agent: {} ({})\n\n{}\n\n- type: {}\n- created_at: {}\n\n## Prompt\n\n{}\n",
        agent_id, agent_name, input.description, normalized_subagent_type, created_at, input.prompt
    );
    std::fs::write(&manifest.output_file, output_contents).map_err(|error| error.to_string())?;

    let manifest_for_spawn = manifest.clone();
    let job = AgentJob {
        manifest: manifest_for_spawn,
        prompt: input.prompt,
        system_prompt,
        allowed_tools,
        tx: TELEMETRY_SINK.get().cloned(),
    };
    if let Err(error) = spawn_fn(job) {
        let error = format!("failed to spawn sub-agent: {error}");
        let _ = persist_agent_terminal_state(&manifest, "failed", None, Some(error.clone()));
        return Err(error);
    }

    emit_telemetry(json!({
        "type": "SubAgentSpawned",
        "agent_id": agent_id,
        "name": agent_name,
        "description": input.description,
        "subagent_type": normalized_subagent_type,
        "status": "running"
    }));

    Ok(manifest)
}

fn spawn_agent_job(job: AgentJob) -> Result<(), String> {
    let token = CancellationToken::new();
    let token_for_task = token.clone();
    
    {
        let mut registry = agent_registry().lock().unwrap();
        registry.insert(job.manifest.agent_id.clone(), token);
    }

    tokio::spawn(async move {
        let agent_id = job.manifest.agent_id.clone();
        
        let result = tokio::select! {
            res = run_agent_job(&job) => res,
            () = token_for_task.cancelled() => {
                Err("Agent cancelled by user or system.".to_string())
            }
        };

        {
            let mut registry = agent_registry().lock().unwrap();
            registry.remove(&agent_id);
        }

        match result {
            Ok(summary) => {
                let _ = persist_agent_terminal_state(&job.manifest, "completed", Some(summary), None);
                
                emit_telemetry(json!({
                    "type": "SubAgentComplete",
                    "agent_id": job.manifest.agent_id,
                    "status": "completed"
                }));
            }
            Err(error) => {
                let _ = persist_agent_terminal_state(&job.manifest, "failed", None, Some(error.clone()));

                emit_telemetry(json!({
                    "type": "SubAgentComplete",
                    "agent_id": job.manifest.agent_id,
                    "status": "failed",
                    "error": error.clone()
                }));
            }
        }
    });

    Ok(())
}

pub(crate) async fn run_agent_job(job: &AgentJob) -> Result<String, String> {
    let mut runtime = build_agent_runtime(job)?.with_max_iterations(DEFAULT_AGENT_MAX_ITERATIONS);
    let summary = runtime
        .run_turn(job.prompt.clone(), None)
        .await
        .map_err(|error| {
            let session_path = std::path::PathBuf::from(format!(".kla/sessions/session-{}.json", job.manifest.agent_id));
            let _ = std::fs::create_dir_all(".kla/sessions");
            let _ = runtime.session().save_to_path(&session_path);
            error.to_string()
        })?;

    let session_path = std::path::PathBuf::from(format!(".kla/sessions/session-{}.json", job.manifest.agent_id));
    let _ = std::fs::create_dir_all(".kla/sessions");
    let _ = runtime.session().save_to_path(&session_path);

    let final_text = final_assistant_text(&summary);
    let mut output = std::fs::OpenOptions::new()
        .append(true)
        .open(&job.manifest.output_file)
        .map_err(|error| error.to_string())?;

    use std::io::Write;
    writeln!(output, "\n## Final Answer\n\n{final_text}").map_err(|error| error.to_string())?;

    Ok(final_text)
}

fn build_agent_runtime(
    job: &AgentJob,
) -> Result<ConversationRuntime<ProviderRuntimeClient, SubagentToolExecutor>, String> {
    let agent_id = job.manifest.agent_id.clone();
    let model = job
        .manifest
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_AGENT_MODEL.to_string());
    let allowed_tools = job.allowed_tools.clone();
    let api_client = ProviderRuntimeClient::new(agent_id.clone(), model, allowed_tools.clone())?;
    let tool_executor = SubagentToolExecutor::new(agent_id, allowed_tools, job.tx.clone());
    Ok(ConversationRuntime::new(
        Session::new(),
        api_client,
        tool_executor,
        agent_permission_policy(),
        job.system_prompt.clone(),
    ))
}

pub fn persist_agent_terminal_state(
    manifest: &AgentOutput,
    status: &str,
    _result: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    let mut updated = manifest.clone();
    updated.status = status.to_string();
    updated.error = error;
    write_agent_manifest(&updated)
}

fn write_agent_manifest(manifest: &AgentOutput) -> Result<(), String> {
    let content = serde_json::to_string_pretty(manifest).map_err(|error| error.to_string())?;
    std::fs::write(&manifest.manifest_file, content).map_err(|error| error.to_string())?;
    Ok(())
}

fn normalize_agent_type(raw: &str) -> String {
    let mut canonical = raw.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();
    if let Some(stripped) = canonical.strip_suffix("tool") {
        canonical = stripped.to_string();
    }
    canonical
}

pub fn allowed_tools_for_type(subagent_type: &str, explicit: Option<Vec<String>>) -> BTreeSet<String> {
    if let Some(tools) = explicit {
        return tools.into_iter().collect();
    }
    let base = match subagent_type {
        "Engineer" => vec![
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "glob_search",
            "grep_search",
            "DiscoveryWorld",
            "SymbolWorld",
        ],
        "Researcher" => vec![
            "WebFetch",
            "WebSearch",
            "read_file",
            "glob_search",
            "grep_search",
            "DiscoveryWorld",
        ],
        _ => vec!["read_file", "glob_search", "grep_search"],
    };
    base.into_iter().map(std::string::ToString::to_string).collect()
}

pub fn agent_permission_policy() -> runtime::PermissionPolicy {
    runtime::PermissionPolicy::new(PermissionMode::WorkspaceWrite)
}

pub(crate) fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

// ── Provider API client for sub-agents ───────────────────────────────

#[derive(Clone)]
pub(crate) struct ProviderRuntimeClient {
    client: ProviderClient,
    model: String,
    allowed_tools: BTreeSet<String>,
    agent_id: String,
}

impl ProviderRuntimeClient {
    pub fn new(agent_id: String, model: String, allowed_tools: BTreeSet<String>) -> Result<Self, String> {
        let model = resolve_model_alias(&model).clone();
        let client = ProviderClient::from_model(&model).map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            model,
            allowed_tools,
            agent_id,
        })
    }
}

#[async_trait::async_trait]
impl ApiClient for ProviderRuntimeClient {
    async fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let tools = tool_specs_for_allowed_tools(Some(&self.allowed_tools))
            .into_iter()
            .map(|spec| ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            })
            .collect::<Vec<_>>();
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: (!tools.is_empty()).then_some(tools),
            tool_choice: (!self.allowed_tools.is_empty()).then_some(ToolChoice::Auto),
            force_json_schema: None,
            stream: true,
        };

        let mut stream = self
            .client
            .stream_message(&message_request)
            .await
            .map_err(|error| RuntimeError::new(error.to_string()))?;
        let mut events = Vec::new();
        let mut pending_tools: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
        let mut saw_stop = false;

        while let Some(event) = stream
            .next_event()
            .await
            .map_err(|error| RuntimeError::new(error.to_string()))?
        {
            match event {
                ApiStreamEvent::MessageStart(start) => {
                    for block in start.message.content {
                        push_output_block(block, 0, &mut events, &mut pending_tools, true);
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    push_output_block(
                        start.content_block,
                        start.index,
                        &mut events,
                        &mut pending_tools,
                        true,
                    );
                }
                ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if !text.is_empty() {
                            emit_telemetry(json!({
                                "type": "SubAgentDelta",
                                "agent_id": self.agent_id,
                                "text": text
                            }));
                            events.push(AssistantEvent::TextDelta(text));
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, _, input)) = pending_tools.get_mut(&delta.index) {
                            if input == "{}" {
                                input.clear();
                            }
                            input.push_str(&partial_json);
                        }
                    }
                    _ => {}
                },
                ApiStreamEvent::ContentBlockStop(stop) => {
                    if let Some((id, name, input)) = pending_tools.remove(&stop.index) {
                        events.push(AssistantEvent::ToolUse { id, name, input });
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    let usage = delta.usage;
                    events.push(AssistantEvent::Usage(TokenUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        cache_read_input_tokens: usage.cache_read_input_tokens,
                    }));
                }
                ApiStreamEvent::MessageStop(_) => {
                    saw_stop = true;
                }
                _ => {}
            }
        }

        if !saw_stop
            && events.iter().any(|event| {
                matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                    || matches!(event, AssistantEvent::ToolUse { .. })
            })
        {
            events.push(AssistantEvent::MessageStop);
        }

        Ok(events)
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }
}

// ── Sub-agent tool executor ──────────────────────────────────────────

pub(crate) struct SubagentToolExecutor {
    allowed_tools: BTreeSet<String>,
    agent_id: String,
    tx: Option<broadcast::Sender<String>>,
}

impl SubagentToolExecutor {
    pub fn new(agent_id: String, allowed_tools: BTreeSet<String>, tx: Option<broadcast::Sender<String>>) -> Self {
        Self { allowed_tools, agent_id, tx }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for SubagentToolExecutor {
    async fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if !self.allowed_tools.contains(tool_name) {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled for this sub-agent"
            )));
        }
        let input_summary = if input.len() > 120 {
            format!("{}…", &input[..120])
        } else {
            input.to_string()
        };
        emit_telemetry(json!({
            "type": "SubAgentToolUse",
            "agent_id": self.agent_id,
            "tool_name": tool_name,
            "input_summary": input_summary
        }));
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let result = execute_tool(tool_name, &value).await.map_err(ToolError::new);

        // High-Fidelity Delta Visualization: Emit current diff if it's a mutation tool
        if matches!(tool_name, "bash" | "write_file" | "edit_file" | "NotebookEdit") {
            if let Some(tx) = &self.tx {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Ok(cp) = runtime::workspace::checkpoint::WorkspaceCheckpoint::new(".").await {
                        if let Ok(diff) = cp.get_current_diff().await {
                             let _ = tx.send(json!({
                                "type": "DiffDelta",
                                "diff": diff
                             }).to_string());
                        }
                    }
                });
            }
        }

        result
    }
}

// ── Stream helpers ───────────────────────────────────────────────────

fn push_output_block(
    block: OutputContentBlock,
    index: u32,
    events: &mut Vec<AssistantEvent>,
    pending_tools: &mut BTreeMap<u32, (String, String, String)>,
    _emit_text_telemetry: bool,
) {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            pending_tools.insert(index, (id, name, input.to_string()));
        }
        OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
    }
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<api::InputMessage> {
    messages.iter().map(|m| api::InputMessage::from(m.clone())).collect()
}

fn tool_specs_for_allowed_tools(allowed: Option<&BTreeSet<String>>) -> Vec<ToolSpec> {
    mvp_tool_specs()
        .into_iter()
        .filter(|spec| allowed.is_none_or(|a| a.contains(spec.name)))
        .collect()
}

fn agent_store_dir() -> Result<std::path::PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if let Some(workspace_root) = cwd.ancestors().nth(2) {
        return Ok(workspace_root.join(".kla-agents"));
    }
    Ok(cwd.join(".kla-agents"))
}

fn make_agent_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("agent-{nanos}")
}
