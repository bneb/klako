pub mod dispatch;
mod agent;
mod config;
mod misc;
mod notebook;
mod web;
mod render;
pub mod worlds;

pub use agent::{cancel_all_agents, set_telemetry_sink, emit_telemetry};

use std::collections::{BTreeMap, BTreeSet};

use api::ToolDefinition;
use plugins::PluginTool;
use runtime::{
    edit_file, execute_bash, glob_search, grep_search, read_file, write_file, BashCommandInput,
    GrepSearchInput, PermissionMode, EditFileInput,
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

#[derive(Clone)]
pub struct GlobalToolRegistry {
    plugin_tools: Vec<PluginTool>,
}

impl GlobalToolRegistry {
    #[must_use] 
    pub fn new(plugin_tools: Vec<PluginTool>) -> Self {
        Self { plugin_tools }
    }

    pub fn with_plugin_tools(plugin_tools: Vec<PluginTool>) -> Result<Self, String> {
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
                let perm_str = tool.required_permission();
                let perm = match perm_str {
                    "workspace-write" => plugins::PluginToolPermission::WorkspaceWrite,
                    "danger-full-access" => plugins::PluginToolPermission::DangerFullAccess,
                    _ => plugins::PluginToolPermission::ReadOnly,
                };
                (
                    tool.definition().name.clone(),
                    permission_mode_from_plugin(perm),
                )
            });
        builtin.chain(plugin).collect()
    }

    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        if mvp_tool_specs().iter().any(|spec| spec.name == name) {
            return execute_tool(name, input).await;
        }
        self.plugin_tools
            .iter()
            .find(|tool| tool.definition().name == name)
            .ok_or_else(|| format!("unsupported tool: {name}"))?
            .execute(input)
            .map_err(|error| error.to_string())
    }
}

#[must_use] 
pub fn permission_mode_from_plugin(p: plugins::PluginToolPermission) -> PermissionMode {
    match p {
        plugins::PluginToolPermission::ReadOnly => PermissionMode::ReadOnly,
        plugins::PluginToolPermission::WorkspaceWrite => PermissionMode::WorkspaceWrite,
        plugins::PluginToolPermission::DangerFullAccess => PermissionMode::DangerFullAccess,
    }
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
}

#[must_use]
pub fn mvp_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash",
            description: "Execute a bash command in the project environment.",
            input_schema: schema_for!(BashCommandInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "read_file",
            description: "Read the contents of a file.",
            input_schema: schema_for!(ReadFileInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "write_file",
            description: "Write the contents of a file.",
            input_schema: schema_for!(WriteFileInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "edit_file",
            description: "Edit a file using a semantic replacement pattern.",
            input_schema: schema_for!(EditFileInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "glob_search",
            description: "Find files matching a glob pattern.",
            input_schema: schema_for!(GlobSearchInputValue).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "grep_search",
            description: "Search for a pattern in file contents.",
            input_schema: schema_for!(GrepSearchInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebFetch",
            description: "Fetch the content of a URL.",
            input_schema: schema_for!(web::WebFetchInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebSearch",
            description: "Search the web for information.",
            input_schema: schema_for!(web::WebSearchInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "DiscoveryWorld",
            description: "Perform codebase analysis and dependency discovery.",
            input_schema: schema_for!(worlds::DiscoveryWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "SymbolWorld",
            description: "Query exact symbol definitions across the codebase.",
            input_schema: schema_for!(worlds::SymbolWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "MemoryWorld",
            description: "Interact with the long-term memory store.",
            input_schema: schema_for!(worlds::MemoryWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "ParityWorld",
            description: "Analyze code for stylistic and naming parity.",
            input_schema: schema_for!(worlds::ParityWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "TemporalWorld",
            description: "Reason about time and scheduling.",
            input_schema: schema_for!(worlds::TemporalWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "LogisticsWorld",
            description: "Solve logistical and mapping problems.",
            input_schema: schema_for!(worlds::LogisticsWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "LiveWorld",
            description: "Interact with real-time live environment data.",
            input_schema: schema_for!(worlds::LiveWorldInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Delegate",
            description: "Spawn a specialized sub-agent for a background task.",
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
        ToolSpec {
            name: "TodoWrite",
            description: "Write a task to the project's TODO list.",
            input_schema: schema_for!(misc::TodoWriteInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Skill",
            description: "Invoke a specialized skill or persona.",
            input_schema: schema_for!(misc::SkillInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "ToolSearch",
            description: "Search for available tools and their definitions.",
            input_schema: schema_for!(misc::ToolSearchInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "NotebookEdit",
            description: "Modify the project's interactive notebook.",
            input_schema: schema_for!(notebook::NotebookEditInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Sleep",
            description: "Pause execution for a specified duration.",
            input_schema: schema_for!(misc::SleepInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "SendUserMessage",
            description: "Send a direct message to the user.",
            input_schema: schema_for!(misc::BriefInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "StructuredOutput",
            description: "Generate structured JSON output.",
            input_schema: schema_for!(misc::StructuredOutputInput).into(),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "REPL",
            description: "Execute interactive commands in a REPL environment.",
            input_schema: schema_for!(misc::ReplInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "PowerShell",
            description: "Execute a PowerShell command.",
            input_schema: schema_for!(misc::PowerShellInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Config",
            description: "Manage project configuration.",
            input_schema: schema_for!(config::ConfigInput).into(),
            required_permission: PermissionMode::WorkspaceWrite,
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
    let seed = input.get("seed").and_then(serde_json::Value::as_u64).unwrap_or_else(rand::random);
    
    let (mut result, artifact) = f(seed)?;
    
    if let Some(obj) = result.as_object_mut() {
        obj.insert("_metadata".to_string(), json!({
            "execution_time_ns": start.elapsed().as_nanos(),
            "prng_seed": seed,
            "artifact": artifact
        }));
    }

    if let Some(art) = &artifact {
        if std::env::var("KLAKO_ENV").unwrap_or_default() == "browser" {
            emit_telemetry(json!({
                "type": "VisualArtifact",
                "artifact": art
            }));
        } else {
            let rendered = render::terminal::render_terminal_artifact(art);
            eprintln!("{rendered}");
        }
    }

    to_pretty_json(result)
}

pub async fn execute_tool(name: &str, input: &Value) -> Result<String, String> {
    dispatch::execute_builtin_tool(name, input).await
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

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadFileInput {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WriteFileInput {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GlobSearchInputValue {
    pattern: String,
    path: Option<String>,
}

async fn run_bash(input: BashCommandInput) -> Result<String, String> {
    let output = execute_bash(input).await.map_err(|error: std::io::Error| error.to_string())?;
    
    if let Some(ref code_msg) = output.return_code_interpretation {
        if code_msg.starts_with("exit_code:") || code_msg == "timeout" {
            let mut err_msg = format!("Command failed: {code_msg}");
            let stdout_trimmed = output.stdout.trim();
            let stderr_trimmed = output.stderr.trim();
            if !stderr_trimmed.is_empty() {
                err_msg.push_str(&format!("\n[stderr]\n{stderr_trimmed}"));
            }
            if !stdout_trimmed.is_empty() {
                err_msg.push_str(&format!("\n[stdout]\n{stdout_trimmed}"));
            }
            return Err(err_msg);
        }
    }

    to_pretty_json(output)
}

fn run_read_file(input: ReadFileInput) -> Result<String, String> {
    to_pretty_json(read_file(&input.path, input.offset, input.limit).map_err(io_to_string)?)
}

fn run_write_file(input: WriteFileInput) -> Result<String, String> {
    to_pretty_json(write_file(&input.path, &input.content).map_err(io_to_string)?)
}

fn run_edit_file(input: EditFileInput) -> Result<String, String> {
    to_pretty_json(
        edit_file(&input.path, &input.old_string, &input.new_string, input.replace_all.unwrap_or(false)).map_err(io_to_string)?,
    )
}

fn run_glob_search(input: GlobSearchInputValue) -> Result<String, String> {
    to_pretty_json(glob_search(&input.pattern, input.path.as_deref()).map_err(io_to_string)?)
}

fn run_grep_search(input: GrepSearchInput) -> Result<String, String> {
    to_pretty_json(grep_search(&input).map_err(io_to_string)?)
}

#[must_use] 
pub fn normalize_tool_name(raw_name: &str) -> String {
    let mut canonical = raw_name.to_lowercase();
    if let Some(stripped) = canonical.strip_suffix("_world") {
        canonical = stripped.to_string();
    }
    canonical
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod lib_tests;
