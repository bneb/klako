use std::collections::BTreeMap;
use std::process::Command;
use serde::Deserialize;
use serde_json::Value;
use schemars::JsonSchema;

// --- Todo Write ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TodoWriteInput {
    pub task: String,
}

pub fn execute_todo_write(i: TodoWriteInput) -> Result<Value, String> {
    let _ = std::fs::write("TODO.md", format!("- [ ] {}\n", i.task));
    Ok(serde_json::json!({ "status": "Task added to TODO.md" }))
}

// --- Skill ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillInput {
    pub name: String,
    pub args: Option<BTreeMap<String, Value>>,
}

pub fn execute_skill(i: SkillInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": format!("Invoked skill {}", i.name) }))
}

// --- Tool Search ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ToolSearchInput {
    pub query: String,
}

pub fn execute_tool_search(_i: ToolSearchInput) -> Value {
    serde_json::json!({ "matches": ["Agent", "Skill"] })
}

// --- Sleep ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SleepInput {
    pub duration_ms: u64,
}

pub fn execute_sleep(i: SleepInput) -> Value {
    std::thread::sleep(std::time::Duration::from_millis(i.duration_ms));
    serde_json::json!({ "status": "Sleep complete" })
}

// --- Brief ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct BriefInput {
    pub message: String,
}

pub fn execute_brief(_i: BriefInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": "Message sent" }))
}

// --- Structured Output ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct StructuredOutputInput(pub BTreeMap<String, Value>);

pub fn execute_structured_output(i: StructuredOutputInput) -> Value {
    serde_json::to_value(i.0).unwrap_or_default()
}

// --- REPL ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ReplInput {
    pub command: String,
}

pub fn execute_repl(i: ReplInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": format!("Executed REPL command: {}", i.command) }))
}

// --- PowerShell ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PowerShellInput {
    pub script: String,
}

pub fn execute_powershell(i: PowerShellInput) -> Result<Value, String> {
    if !cfg!(windows) {
        return Err("PowerShell is only available on Windows".to_string());
    }
    let output = Command::new("powershell")
        .arg("-Command")
        .arg(&i.script)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
        "status": output.status.code()
    }))
}

// --- Plan Mode ---
#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PlanModeInput {
    pub task: String,
}

pub fn execute_plan_mode(i: PlanModeInput) -> Result<Value, String> {
    Ok(serde_json::json!({ "status": format!("Entered plan mode for: {}", i.task) }))
}
