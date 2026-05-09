use std::io::Write;
use std::env;
use std::path::{Path, PathBuf};
use api::{InputMessage, StreamEvent as ApiStreamEvent};
use runtime::{
    AssistantEvent, ContentBlock, ConversationMessage, ConversationRuntime, 
    RuntimeError, TokenUsage, Session, ConfigLoader, PermissionMode,
};
use tools::GlobalToolRegistry;
use super::client::DefaultRuntimeClient;
use super::executor::CliToolExecutor;
use super::progress::InternalPromptProgressReporter;
use super::permission_policy;
use plugins::{PluginManager, PluginManagerConfig};

pub async fn build_runtime(
    session: Session,
    model: String,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<crate::AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
    tx: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>, Box<dyn std::error::Error>>
{
    let (feature_config, tool_registry) = build_runtime_plugin_state()?;
    Ok(ConversationRuntime::new_with_features(
        session,
        DefaultRuntimeClient::new(
            model,
            enable_tools,
            emit_output,
            allowed_tools.clone(),
            tool_registry.clone(),
            progress_reporter,
            feature_config.clone(),
            tx,
            system_prompt.clone(),
        ).await?,
        CliToolExecutor::new(allowed_tools.clone(), emit_output, tool_registry.clone()),
        permission_policy(permission_mode, &tool_registry),
        system_prompt,
        feature_config,
    ))
}

pub fn build_runtime_plugin_state(
) -> Result<(runtime::RuntimeFeatureConfig, GlobalToolRegistry), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    let plugin_manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let tool_registry = GlobalToolRegistry::with_plugin_tools(plugin_manager.aggregated_tools()?)?;
    Ok((runtime_config.feature_config().clone(), tool_registry))
}

#[must_use] 
pub fn build_plugin_manager(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

fn resolve_plugin_path(cwd: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

pub fn push_output_block(
    block: api::OutputContentBlock,
    _out: &mut dyn Write,
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    _emit_output: bool,
) -> Result<(), RuntimeError> {
    match block {
        api::OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        api::OutputContentBlock::ToolUse { id, name, input } => {
            let input_str = input.to_string();
            *pending_tool = Some((id, name, input_str));
        }
        api::OutputContentBlock::Thinking { .. } | api::OutputContentBlock::RedactedThinking { .. } => {}
    }
    Ok(())
}

#[must_use] 
pub fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages.iter().map(|m| InputMessage::from(m.clone())).collect()
}

#[must_use] 
pub fn stream_event_to_assistant_event(event: ApiStreamEvent) -> Option<AssistantEvent> {
    match event {
        ApiStreamEvent::ContentBlockDelta(delta) => {
            match delta.delta {
                api::ContentBlockDelta::TextDelta { text } => Some(AssistantEvent::TextDelta(text)),
                api::ContentBlockDelta::InputJsonDelta { partial_json } => {
                    Some(AssistantEvent::TextDelta(partial_json)) 
                }
                _ => None
            }
        }
        ApiStreamEvent::MessageDelta(delta) => {
            let usage = delta.usage;
            Some(AssistantEvent::Usage(TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
            }))
        }
        ApiStreamEvent::MessageStop(_) => Some(AssistantEvent::MessageStop),
        _ => None,
    }
}

#[must_use] 
pub fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
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

#[must_use] 
pub fn summarize_tool_payload(input: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(obj) = v.as_object() {
            if let Some(cmd) = obj.get("command").and_then(|v| v.as_str()) {
                return truncate(cmd, 60);
            }
            if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                return truncate(path, 60);
            }
        }
    }
    truncate(input, 60)
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        s
    } else {
        format!("{}…", &s[..max - 1])
    }
}
