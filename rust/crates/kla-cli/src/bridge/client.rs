use std::io::{self, Write};
use api::{
    max_tokens_for_model, AuthSource, MessageRequest, ToolChoice,
    StreamEvent as ApiStreamEvent,
};
use runtime::{
    ApiClient, ApiRequest, AssistantEvent, RuntimeError,
};
use tools::GlobalToolRegistry;
use crate::render::{MarkdownStreamState, TerminalRenderer};
use super::progress::InternalPromptProgressReporter;
use super::helpers::{convert_messages, push_output_block};

#[derive(Clone)]
pub struct DefaultRuntimeClient {
    pub(crate) router: api::router::Router,
    pub(crate) model: String,
    pub(crate) enable_tools: bool,
    pub(crate) emit_output: bool,
    pub(crate) allowed_tools: Option<crate::AllowedToolSet>,
    pub(crate) tool_registry: GlobalToolRegistry,
    pub(crate) progress_reporter: Option<InternalPromptProgressReporter>,
    pub(crate) tx: Option<tokio::sync::broadcast::Sender<String>>,
    pub(crate) system_prompt: Vec<String>,
}

impl DefaultRuntimeClient {
    pub async fn new(
        model: String,
        enable_tools: bool,
        emit_output: bool,
        allowed_tools: Option<crate::AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        progress_reporter: Option<InternalPromptProgressReporter>,
        feature_config: runtime::RuntimeFeatureConfig,
        tx: Option<tokio::sync::broadcast::Sender<String>>,
        system_prompt: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let auth = match crate::bridge::resolve_cli_auth_source().await {
            Ok(auth) => auth,
            Err(e) if feature_config.agency_topology().is_some() => {
                println!("Warning: No Klako cloud credentials found in environment or active config.");
                println!("Proceeding without cloud credentials. Any cloud-bound routing will fail unless topology defaults to local models.");
                AuthSource::None
            }
            Err(e) => return Err(e),
        };
        
        let _client = api::ProviderClient::from_model_with_default_auth(&model, Some(auth))?
            .with_base_url(api::read_base_url());

        let router = if let Some(topology) = feature_config.agency_topology() {
            api::router::build_router_from_topology(topology)?
        } else {
            // Default to single-model router if no topology
            return Err("Legacy router building not implemented in bridge/client.rs".into());
        };

        Ok(Self {
            router,
            model,
            enable_tools,
            emit_output,
            allowed_tools,
            tool_registry,
            progress_reporter,
            tx,
            system_prompt,
        })
    }
}

#[async_trait::async_trait]
impl ApiClient for DefaultRuntimeClient {
    #[allow(clippy::too_many_lines)]
    async fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if let Some(progress_reporter) = &self.progress_reporter {
            progress_reporter.mark_model_phase();
        }
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!self.system_prompt.is_empty()).then(|| self.system_prompt.join("\n\n")),
            tools: self
                .enable_tools
                .then(|| filter_tool_specs(&self.tool_registry, self.allowed_tools.as_ref())),
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            force_json_schema: None,
            stream: true,
        };

        let stream_events = self
            .router
            .stream_with_escalation(&message_request)
            .await
            .map_err(|error| RuntimeError::new(error.to_string()))?;
        
        let mut stdout = io::stdout();
        let mut sink = io::sink();
        let out: &mut dyn Write = if self.emit_output {
            &mut stdout
        } else {
            &mut sink
        };
        let _renderer = TerminalRenderer::new();
        let mut _markdown_stream = MarkdownStreamState::default();
        let mut events = Vec::new();
        let mut pending_tool: Option<(String, String, String)> = None;
        let mut _saw_stop = false;

        for event in stream_events {
            match event {
                ApiStreamEvent::MessageStart(start) => {
                    if let Some(tx) = &self.tx {
                        let payload = serde_json::json!({
                            "type": "StatusUpdate",
                            "role": "thinker",
                            "tier": "L0_Thinker // Reasoning"
                        });
                        let _ = tx.send(payload.to_string());
                    }
                    let _ = writeln!(out, "\n\x1b[38;5;238m╭─\x1b[0m \x1b[1;38;5;45m[L0_Thinker]\x1b[0m \x1b[38;5;238m──────────────────────────────────────────╮\x1b[0m");
                    for block in start.message.content {
                        push_output_block(block, out, &mut events, &mut pending_tool, true)?;
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    push_output_block(start.content_block, out, &mut events, &mut pending_tool, true)?;
                }
                ApiStreamEvent::ContentBlockDelta(delta) => {
                    if let api::ContentBlockDelta::TextDelta { text } = delta.delta {
                         if !text.is_empty() {
                            if let Some(tx) = &self.tx {
                                let payload = serde_json::json!({
                                    "type": "NarrativeDelta",
                                    "role": "thinker",
                                    "tier": "L0_Thinker",
                                    "text": text
                                });
                                let _ = tx.send(payload.to_string());
                            }
                            events.push(AssistantEvent::TextDelta(text));
                        }
                    }
                }
                ApiStreamEvent::ContentBlockStop(_) => {
                    if let Some((id, name, input)) = pending_tool.take() {
                        events.push(AssistantEvent::ToolUse { id, name, input });
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    let usage = delta.usage;
                    events.push(AssistantEvent::Usage(runtime::TokenUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        cache_read_input_tokens: usage.cache_read_input_tokens,
                    }));
                }
                ApiStreamEvent::MessageStop(_) => {
                    _saw_stop = true;
                }
                _ => {}
            }
        }
        
        let _ = writeln!(out, "\x1b[38;5;238m╰────────────────────────────────────────────────────────────────────────────╯\x1b[0m");
        Ok(events)
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }
}

fn filter_tool_specs(
    registry: &GlobalToolRegistry,
    allowed: Option<&crate::AllowedToolSet>,
) -> Vec<api::ToolDefinition> {
    registry.definitions(allowed)
}
