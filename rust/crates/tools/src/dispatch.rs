use serde_json::{from_value, Value};
use crate::{
    to_pretty_json, worlds, web, misc, notebook, agent, config,
    BashCommandInput, ReadFileInput, WriteFileInput, EditFileInput,
    GlobSearchInputValue, GrepSearchInput,
    run_bash, run_read_file, run_write_file, run_edit_file,
    run_glob_search, run_grep_search, with_metadata,
};

pub async fn execute_builtin_tool(name: &str, input: &Value) -> Result<String, String> {
    match name {
        // Core Filesystem & Shell
        "bash" => run_bash(from_value::<BashCommandInput>(input.clone()).map_err(|e| e.to_string())?).await,
        
        // Blocking Tools (Wrapped in spawn_blocking)
        "read_file" => {
            let i = from_value::<ReadFileInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || run_read_file(i)).await.map_err(|e| e.to_string())?
        },
        "write_file" => {
            let i = from_value::<WriteFileInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || run_write_file(i)).await.map_err(|e| e.to_string())?
        },
        "edit_file" => {
            let i = from_value::<EditFileInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || run_edit_file(i)).await.map_err(|e| e.to_string())?
        },
        "glob_search" => {
            let i = from_value::<GlobSearchInputValue>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || run_glob_search(i)).await.map_err(|e| e.to_string())?
        },
        "grep_search" => {
            let i = from_value::<GrepSearchInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || run_grep_search(i)).await.map_err(|e| e.to_string())?
        },
        
        // Web & External (Blocking)
        "WebFetch" => {
            let i = from_value::<web::WebFetchInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || to_pretty_json(web::execute_web_fetch(&i)?)).await.map_err(|e| e.to_string())?
        },
        "WebSearch" => {
            let i = from_value::<web::WebSearchInput>(input.clone()).map_err(|e| e.to_string())?;
            tokio::task::spawn_blocking(move || to_pretty_json(web::execute_web_search(&i)?)).await.map_err(|e| e.to_string())?
        },
            
        // Agents & Orchestration
        "Agent" | "Delegate" => {
            let i = from_value::<agent::AgentInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(agent::execute_agent(i)?)
        },
            
        // Notebook & UI
        "NotebookEdit" => {
            let i = from_value::<notebook::NotebookEditInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(notebook::execute_notebook_edit(i)?)
        },
            
        // Worlds (Domain Kernels)
        "DiscoveryWorld" => {
            let i = from_value::<worlds::DiscoveryWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_discovery_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "SymbolWorld" => {
            let i = from_value::<worlds::SymbolWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_symbol_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "MemoryWorld" => {
            let i = from_value::<worlds::MemoryWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_memory_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "ParityWorld" => {
            let i = from_value::<worlds::ParityWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_parity_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "TemporalWorld" => {
            let i = from_value::<worlds::TemporalWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_temporal_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "LogisticsWorld" => {
            let i = from_value::<worlds::LogisticsWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_logistics_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
        "LiveWorld" => {
            let i = from_value::<worlds::LiveWorldInput>(input.clone()).map_err(|e| e.to_string())?;
            let input_captured = input.clone();
            tokio::task::spawn_blocking(move || {
                with_metadata(&input_captured, |_seed| Ok((worlds::execute_live_world(i)?, None)))
            }).await.map_err(|e| e.to_string())?
        },
            
        // Misc Utilities
        "TodoWrite" => {
            let i = from_value::<misc::TodoWriteInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_todo_write(i)?)
        },
        "Skill" => {
            let i = from_value::<misc::SkillInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_skill(i)?)
        },
        "enter_plan_mode" => {
            let i = from_value::<misc::PlanModeInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_plan_mode(i)?)
        },
        "ToolSearch" => {
            let i = from_value::<misc::ToolSearchInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_tool_search(i))
        },
        "Sleep" => {
            let i = from_value::<misc::SleepInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_sleep(i))
        },
        "SendUserMessage" | "Brief" => {
            let i = from_value::<misc::BriefInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_brief(i)?)
        },
        "StructuredOutput" => {
            let i = from_value::<misc::StructuredOutputInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_structured_output(i))
        },
        "REPL" => {
            let i = from_value::<misc::ReplInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_repl(i)?)
        },
        "PowerShell" => {
            let i = from_value::<misc::PowerShellInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(misc::execute_powershell(i).map_err(|e| e.clone())?)
        },
        "Config" => {
            let i = from_value::<config::ConfigInput>(input.clone()).map_err(|e| e.to_string())?;
            to_pretty_json(config::execute_config(i)?)
        },
            
        _ => Err(format!("unsupported tool: {name}")),
    }
}
