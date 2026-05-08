pub mod agent;
mod config;
mod misc;
mod notebook;
mod web;
mod render;
mod worlds;

pub use agent::set_telemetry_sink;

use std::collections::{BTreeMap, BTreeSet};

use api::ToolDefinition;
use plugins::PluginTool;
use runtime::{
    edit_file, execute_bash, glob_search, grep_search, read_file, write_file, BashCommandInput,
    GrepSearchInput, PermissionMode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use schemars::{schema_for, JsonSchema};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolManifestEntry {
    pub name: String,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Base,
    Conditional,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolRegistry {
    entries: Vec<ToolManifestEntry>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new(entries: Vec<ToolManifestEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn entries(&self) -> &[ToolManifestEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalToolRegistry {
    plugin_tools: Vec<PluginTool>,
}

impl GlobalToolRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            plugin_tools: Vec::new(),
        }
    }

    pub fn with_plugin_tools(plugin_tools: Vec<PluginTool>) -> Result<Self, String> {
        let builtin_names = mvp_tool_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect::<BTreeSet<_>>();
        let mut seen_plugin_names = BTreeSet::new();

        for tool in &plugin_tools {
            let name = tool.definition().name.clone();
            if builtin_names.contains(&name) {
                return Err(format!(
                    "plugin tool `{name}` conflicts with a built-in tool name"
                ));
            }
            if !seen_plugin_names.insert(name.clone()) {
                return Err(format!("duplicate plugin tool name `{name}`"));
            }
        }

        Ok(Self { plugin_tools })
    }

    pub fn normalize_allowed_tools(
        &self,
        values: &[String],
    ) -> Result<Option<BTreeSet<String>>, String> {
        if values.is_empty() {
            return Ok(None);
        }

        let builtin_specs = mvp_tool_specs();
        let canonical_names = builtin_specs
            .iter()
            .map(|spec| spec.name.to_string())
            .chain(
                self.plugin_tools
                    .iter()
                    .map(|tool| tool.definition().name.clone()),
            )
            .collect::<Vec<_>>();
        let mut name_map = canonical_names
            .iter()
            .map(|name| (normalize_tool_name(name), name.clone()))
            .collect::<BTreeMap<_, _>>();

        for (alias, canonical) in [
            ("read", "read_file"),
            ("write", "write_file"),
            ("edit", "edit_file"),
            ("glob", "glob_search"),
            ("grep", "grep_search"),
        ] {
            name_map.insert(alias.to_string(), canonical.to_string());
        }

        let mut allowed = BTreeSet::new();
        for value in values {
            for token in value
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|token| !token.is_empty())
            {
                let normalized = normalize_tool_name(token);
                let canonical = name_map.get(&normalized).ok_or_else(|| {
                    format!(
                        "unsupported tool in --allowedTools: {token} (expected one of: {})",
                        canonical_names.join(", ")
                    )
                })?;
                allowed.insert(canonical.clone());
            }
        }

        Ok(Some(allowed))
    }

    #[must_use]
    pub fn definitions(&self, allowed_tools: Option<&BTreeSet<String>>) -> Vec<ToolDefinition> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            });
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| ToolDefinition {
                name: tool.definition().name.clone(),
                description: tool.definition().description.clone(),
                input_schema: tool.definition().input_schema.clone(),
            });
        builtin.chain(plugin).collect()
    }

    #[must_use]
    pub fn permission_specs(
        &self,
        allowed_tools: Option<&BTreeSet<String>>,
    ) -> Vec<(String, PermissionMode)> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| (spec.name.to_string(), spec.required_permission));
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| {
                (
                    tool.definition().name.clone(),
                    permission_mode_from_plugin(tool.required_permission()),
                )
            });
        builtin.chain(plugin).collect()
    }

    pub fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        if mvp_tool_specs().iter().any(|spec| spec.name == name) {
            return execute_tool(name, input);
        }
        self.plugin_tools
            .iter()
            .find(|tool| tool.definition().name == name)
            .ok_or_else(|| format!("unsupported tool: {name}"))?
            .execute(input)
            .map_err(|error| error.to_string())
    }
}

fn normalize_tool_name(value: &str) -> String {
    value.trim().replace('-', "_").to_ascii_lowercase()
}

fn permission_mode_from_plugin(value: &str) -> PermissionMode {
    match value {
        "read-only" => PermissionMode::ReadOnly,
        "workspace-write" => PermissionMode::WorkspaceWrite,
        "danger-full-access" => PermissionMode::DangerFullAccess,
        other => panic!("unsupported plugin permission: {other}"),
    }
}

fn schema_for_type<T: JsonSchema>() -> Value {
    let mut schema = serde_json::to_value(schema_for!(T)).unwrap();
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
    }
    schema
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn mvp_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash",
            description: "Execute a shell command in the current workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout": { "type": "integer", "minimum": 1 },
                    "description": { "type": "string" },
                    "run_in_background": { "type": "boolean" },
                    "dangerouslyDisableSandbox": { "type": "boolean" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "read_file",
            description: "Read a text file from the workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "write_file",
            description: "Write a text file in the workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace text in a workspace file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "replace_all": { "type": "boolean" }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "glob_search",
            description: "Find files by glob pattern.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "grep_search",
            description: "Search file contents with a regex pattern.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "glob": { "type": "string" },
                    "output_mode": { "type": "string" },
                    "-B": { "type": "integer", "minimum": 0 },
                    "-A": { "type": "integer", "minimum": 0 },
                    "-C": { "type": "integer", "minimum": 0 },
                    "context": { "type": "integer", "minimum": 0 },
                    "-n": { "type": "boolean" },
                    "-i": { "type": "boolean" },
                    "type": { "type": "string" },
                    "head_limit": { "type": "integer", "minimum": 1 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "multiline": { "type": "boolean" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebFetch",
            description:
                "Fetch a URL, convert it into readable text, and answer a prompt about it.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "format": "uri" },
                    "prompt": { "type": "string" }
                },
                "required": ["url", "prompt"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebSearch",
            description: "Search the web for current information and return cited results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "minLength": 2 },
                    "allowed_domains": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "blocked_domains": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "TodoWrite",
            description: "Update the structured task list for the current session.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "activeForm": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"]
                                }
                            },
                            "required": ["content", "activeForm", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["todos"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Skill",
            description: "Load a local skill definition and its instructions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "skill": { "type": "string" },
                    "args": { "type": "string" }
                },
                "required": ["skill"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Agent",
            description: "Launch a specialized agent task and persist its handoff metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string" },
                    "prompt": { "type": "string" },
                    "subagent_type": { "type": "string" },
                    "name": { "type": "string" },
                    "model": { "type": "string" },
                    "allowed_tools": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["description", "prompt"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "ToolSearch",
            description: "Search for deferred or specialized tools by exact name or keywords.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "NotebookEdit",
            description: "Replace, insert, or delete a cell in a Jupyter notebook.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "notebook_path": { "type": "string" },
                    "cell_id": { "type": "string" },
                    "new_source": { "type": "string" },
                    "cell_type": { "type": "string", "enum": ["code", "markdown"] },
                    "edit_mode": { "type": "string", "enum": ["replace", "insert", "delete"] }
                },
                "required": ["notebook_path"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Sleep",
            description: "Wait for a specified duration without holding a shell process.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "duration_ms": { "type": "integer", "minimum": 0 }
                },
                "required": ["duration_ms"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "SendUserMessage",
            description: "Send a message to the user.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" },
                    "attachments": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "status": {
                        "type": "string",
                        "enum": ["normal", "proactive"]
                    }
                },
                "required": ["message", "status"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Config",
            description: "Get or set Claw Code settings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "setting": { "type": "string" },
                    "value": {
                        "type": ["string", "boolean", "number"]
                    }
                },
                "required": ["setting"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "StructuredOutput",
            description: "Return structured output in the requested format.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": true
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "REPL",
            description: "Execute code in a REPL-like subprocess.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "language": { "type": "string" },
                    "timeout_ms": { "type": "integer", "minimum": 1 }
                },
                "required": ["code", "language"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "PowerShell",
            description: "Execute a PowerShell command with optional timeout.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout": { "type": "integer", "minimum": 1 },
                    "description": { "type": "string" },
                    "run_in_background": { "type": "boolean" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "MemoryWorld",
            description: "Cross-session memory persistence for global facts and project preferences.",
            input_schema: schema_for_type::<worlds::core::MemoryWorldInput>(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "DiscoveryWorld",
            description: "Contextual Compaction engine for high-speed structural mapping of codebases (AST/Symbol extraction).",
            input_schema: schema_for_type::<worlds::DiscoveryWorldInput>(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "SymbolWorld",
            description: "Lightweight LSP engine for semantic symbol lookup, Go To Definition, and reference finding.",
            input_schema: schema_for_type::<worlds::SymbolWorldInput>(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "ParityWorld",
            description: "Structural parity engine for verifying code idiomaticity, indentation, and naming conventions.",
            input_schema: schema_for_type::<worlds::ParityWorldInput>(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "enter_plan_mode",
            description: "Switch to Plan Mode to safely research, design, and plan complex changes using read-only tools.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string" }
                },
                "required": ["reason"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Delegate",
            description: "Spawns an isolated sub-agent to handle a specialized task.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "subagent_type": { "type": "string" },
                    "description": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["subagent_type", "description", "prompt"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
    ]
}


#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisualArtifact {
    Histogram { title: String, data: Vec<f64>, bins: usize },
    GanttChart { title: String, events: Vec<GanttEvent> },
    Table { title: String, headers: Vec<String>, rows: Vec<Vec<String>> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GanttEvent { pub label: String, pub start: f64, pub end: f64, pub status: String }

fn with_metadata(input: &Value, f: impl FnOnce(u64) -> Result<(Value, Option<VisualArtifact>), String>) -> Result<String, String> {
    let start = std::time::Instant::now();
    let seed = input.get("seed").and_then(|v| v.as_u64()).unwrap_or_else(|| rand::random());
    
    let (mut result, artifact) = f(seed)?;
    
    if let Some(obj) = result.as_object_mut() {
        obj.insert("_metadata".to_string(), json!({
            "execution_time_ns": start.elapsed().as_nanos(),
            "prng_seed": seed,
            "artifact": artifact
        }));
    }

    if let Some(art) = &artifact {
        // Detect if we are in terminal mode. 
        // For Klako, we assume terminal if KLAKO_ENV is not 'browser'.
        if std::env::var("KLAKO_ENV").unwrap_or_default() != "browser" {
            let rendered = render::terminal::render_terminal_artifact(art);
            eprintln!("{}", rendered);
        } else {
            // Emit telemetry for the browser to catch
            agent::emit_telemetry(json!({
                "type": "VisualArtifact",
                "artifact": art
            }));
        }
    }

    to_pretty_json(result)
}

pub fn execute_tool(name: &str, input: &Value) -> Result<String, String> {
    match name {
        "bash" => from_value::<BashCommandInput>(input).and_then(run_bash),
        "read_file" => from_value::<ReadFileInput>(input).and_then(run_read_file),
        "write_file" => from_value::<WriteFileInput>(input).and_then(run_write_file),
        "edit_file" => from_value::<EditFileInput>(input).and_then(run_edit_file),
        "glob_search" => from_value::<GlobSearchInputValue>(input).and_then(run_glob_search),
        "grep_search" => from_value::<GrepSearchInput>(input).and_then(run_grep_search),
        "WebFetch" => from_value::<web::WebFetchInput>(input)
            .and_then(|i| to_pretty_json(web::execute_web_fetch(&i)?)),
        "WebSearch" => from_value::<web::WebSearchInput>(input)
            .and_then(|i| to_pretty_json(web::execute_web_search(&i)?)),
        "TodoWrite" => from_value::<misc::TodoWriteInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_todo_write(i)?)),
        "Skill" => from_value::<misc::SkillInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_skill(i)?)),
        "Agent" => from_value::<agent::AgentInput>(input)
            .and_then(|i| to_pretty_json(agent::execute_agent(i)?)),
        "Delegate" => from_value::<agent::AgentInput>(input)
            .and_then(|i| to_pretty_json(agent::execute_agent(i)?)),
        "enter_plan_mode" => from_value::<misc::PlanModeInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_plan_mode(i)?)),
        "ToolSearch" => from_value::<misc::ToolSearchInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_tool_search(i))),
        "NotebookEdit" => from_value::<notebook::NotebookEditInput>(input)
            .and_then(|i| to_pretty_json(notebook::execute_notebook_edit(i)?)),
        "Sleep" => from_value::<misc::SleepInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_sleep(i))),
        "SendUserMessage" | "Brief" => from_value::<misc::BriefInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_brief(i)?)),
        "StructuredOutput" => from_value::<misc::StructuredOutputInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_structured_output(i))),
        "REPL" => from_value::<misc::ReplInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_repl(i)?)),
        "PowerShell" => from_value::<misc::PowerShellInput>(input)
            .and_then(|i| to_pretty_json(misc::execute_powershell(i).map_err(|e| e.to_string())?)),
        "Config" => from_value::<config::ConfigInput>(input)
            .and_then(|i| to_pretty_json(config::execute_config(i)?)),
        "MemoryWorld" => from_value::<worlds::core::MemoryWorldInput>(input)
            .and_then(|i| with_metadata(input, |_seed| Ok((worlds::execute_memory_world(i)?, None)))),
        "DiscoveryWorld" => from_value::<worlds::DiscoveryWorldInput>(input)
            .and_then(|i| with_metadata(input, |_seed| Ok((worlds::execute_discovery_world(i)?, None)))),
        "SymbolWorld" => from_value::<worlds::SymbolWorldInput>(input)
            .and_then(|i| with_metadata(input, |_seed| Ok((worlds::execute_symbol_world(i)?, None)))),
        "ParityWorld" => from_value::<worlds::ParityWorldInput>(input)
            .and_then(|i| with_metadata(input, |_seed| Ok((worlds::execute_parity_world(i)?, None)))),
        _ => Err(format!("unsupported tool: {name}")),
    }
}

// ── Deserialization & serialization helpers ───────────────────────────

fn from_value<T: for<'de> Deserialize<'de>>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|error| error.to_string())
}

fn to_pretty_json<T: serde::Serialize>(value: T) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn io_to_string(error: std::io::Error) -> String {
    error.to_string()
}

// ── File system tool wrappers ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ReadFileInput {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WriteFileInput {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct EditFileInput {
    path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GlobSearchInputValue {
    pattern: String,
    path: Option<String>,
}

fn run_bash(input: BashCommandInput) -> Result<String, String> {
    let output = execute_bash(input).map_err(|error| error.to_string())?;
    
    // If the command failed with a non-zero exit code, return an Err so the model 
    // sees is_error=true, but make sure to include the actual stderr/stdout in the message!
    if let Some(ref code_msg) = output.return_code_interpretation {
        if code_msg.starts_with("exit_code:") || code_msg == "timeout" {
            let mut err_msg = format!("Command failed -> {code_msg}\n");
            let stderr_trimmed = output.stderr.trim();
            let stdout_trimmed = output.stdout.trim();
            if !stderr_trimmed.is_empty() {
                err_msg.push_str(&format!("\n[stderr]\n{stderr_trimmed}"));
            }
            if !stdout_trimmed.is_empty() {
                err_msg.push_str(&format!("\n[stdout]\n{stdout_trimmed}"));
            }
            return Err(err_msg);
        }
    }

    serde_json::to_string_pretty(&output).map_err(|error| error.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn run_read_file(input: ReadFileInput) -> Result<String, String> {
    to_pretty_json(read_file(&input.path, input.offset, input.limit).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_write_file(input: WriteFileInput) -> Result<String, String> {
    to_pretty_json(write_file(&input.path, &input.content).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_edit_file(input: EditFileInput) -> Result<String, String> {
    to_pretty_json(
        edit_file(
            &input.path,
            &input.old_string,
            &input.new_string,
            input.replace_all.unwrap_or(false),
        )
        .map_err(io_to_string)?,
    )
}

#[allow(clippy::needless_pass_by_value)]
fn run_glob_search(input: GlobSearchInputValue) -> Result<String, String> {
    to_pretty_json(glob_search(&input.pattern, input.path.as_deref()).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_grep_search(input: GrepSearchInput) -> Result<String, String> {
    to_pretty_json(grep_search(&input).map_err(io_to_string)?)
}

include!("lib_tests.rs");
