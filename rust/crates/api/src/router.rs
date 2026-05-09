//! Multi-tiered inference routing topology.
//!
//! The `Router` dispatches inference requests to a role-based L0 pair
//! (thinker for reasoning/chat, typist for tool evaluation) and
//! automatically escalates through a configurable chain of cloud
//! providers when the local tier fails.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use runtime::{AgencyTopology, ProviderEntry};

use crate::error::ApiError;
use crate::providers::openai_compat::{OpenAiCompatClient, OpenAiCompatConfig};
use crate::types::{MessageRequest, StreamEvent};

// ---------------------------------------------------------------------------
// InferenceProvider trait
// ---------------------------------------------------------------------------

/// Abstracts any engine capable of evaluating a message stream.
///
/// This operates at the semantic routing level—"send this conversation state
/// somewhere and get events back"—not at the HTTP transport level.
pub trait InferenceProvider: Send + Sync + dyn_clone::DynClone {
    fn stream_inference<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>>;

    fn provider_label(&self) -> &str;
}

dyn_clone::clone_trait_object!(InferenceProvider);

// ---------------------------------------------------------------------------
// OpenAiCompatInferenceProvider adapter
// ---------------------------------------------------------------------------

/// Wraps the existing `OpenAiCompatClient` to implement `InferenceProvider`.
///
/// Powers both local `llama_cpp` endpoints (which expose OpenAI-compatible
/// `/v1/chat/completions`) and cloud Gemini endpoints (which also use
/// the OpenAI-compat surface).
#[derive(Clone)]
pub struct OpenAiCompatInferenceProvider {
    client: OpenAiCompatClient,
    label: String,
    model: String,
    engine_name: String,
    disable_tools: bool,
}

impl OpenAiCompatInferenceProvider {
    pub fn new(client: OpenAiCompatClient, label: impl Into<String>, model: String, engine_name: String, disable_tools: bool) -> Self {
        Self {
            client,
            label: label.into(),
            model,
            engine_name,
            disable_tools,
        }
    }
}

impl InferenceProvider for OpenAiCompatInferenceProvider {
    fn stream_inference<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>> {
        let mut specialized_request = request.clone();
        specialized_request.model = self.model.clone();
        if self.disable_tools {
            specialized_request.tools = None;
            specialized_request.tool_choice = None;
        } else if self.engine_name == "llama_cpp" {
            // ATLAS Feature Port: Automatically inject `force_json_schema` GBNF constraint 
            // for local `llama_cpp` models so they are structurally bound to proper JSON format.
            if specialized_request.tools.is_some() {
                let schema = serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "arguments": {"type": "object"}
                    },
                    "required": ["name", "arguments"]
                });
                specialized_request.force_json_schema = Some(schema);
            }
        }
        
        Box::pin(async move {
            let mut stream = self.client.stream_message(&specialized_request).await?;
            let mut events = Vec::new();
            while let Some(event) = stream.next_event().await? {
                events.push(event);
            }
            normalize_json_tool_calls(&mut events);
            
            if let Some(refusal) = detect_provider_refusal(&events) {
                return Err(ApiError::ProviderRefusal(refusal));
            }
            Ok(events)
        })
    }

    fn provider_label(&self) -> &str {
        &self.label
    }
}

fn detect_provider_refusal(events: &[StreamEvent]) -> Option<String> {
    let mut full_text = String::new();
    let mut has_tool = false;
    
    for event in events {
        match event {
            StreamEvent::ContentBlockStart(start) => {
                match &start.content_block {
                    crate::types::OutputContentBlock::Text { text } => {
                        full_text.push_str(text);
                    }
                    crate::types::OutputContentBlock::ToolUse { .. } => {
                        has_tool = true;
                    }
                    _ => {}
                }
            }
            StreamEvent::ContentBlockDelta(delta) => {
                if let crate::types::ContentBlockDelta::TextDelta { text } = &delta.delta {
                    full_text.push_str(text);
                }
            }
            _ => {}
        }
    }

    if has_tool {
        return None;
    }

    let lower = full_text.to_lowercase();
    let refusal_patterns = [
        "i'm sorry",
        "i am sorry",
        "as an ai",
        "i am an ai",
        "exceeds the scope",
        "exceeds my capabilities",
        "i cannot fulfill",
        "i am unable to",
        "i can't help with",
        "beyond the capabilities",
    ];

    for pattern in &refusal_patterns {
        if lower.contains(pattern) {
            return Some(full_text.trim().to_string());
        }
    }

    None
}

fn push_normalized_text_block(events: &mut Vec<StreamEvent>, text: &str, index: u32) {
    if text.is_empty() {
        return;
    }
    events.push(StreamEvent::ContentBlockStart(crate::types::ContentBlockStartEvent {
        index,
        content_block: crate::types::OutputContentBlock::Text { text: String::new() }
    }));
    events.push(StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent {
        index,
        delta: crate::types::ContentBlockDelta::TextDelta { text: text.to_string() }
    }));
    events.push(StreamEvent::ContentBlockStop(crate::types::ContentBlockStopEvent { index }));
}

pub fn get_normalized_tool_name(raw_name: &str) -> Result<String, String> {
    let normalized = raw_name.replace(['_', '-'], "").to_ascii_lowercase();
    match normalized.as_str() {
        "writefile" => Ok("write_file".to_string()),
        "readfile" => Ok("read_file".to_string()),
        "editfile" => Ok("edit_file".to_string()),
        "globsearch" => Ok("glob_search".to_string()),
        "grepsearch" => Ok("grep_search".to_string()),
        "websearch" => Ok("WebSearch".to_string()),
        "webfetch" => Ok("WebFetch".to_string()),
        "todowrite" => Ok("TodoWrite".to_string()),
        "toolsearch" => Ok("ToolSearch".to_string()),
        "notebookedit" => Ok("NotebookEdit".to_string()),
        "sendusermessage" => Ok("SendUserMessage".to_string()),
        "structuredoutput" => Ok("StructuredOutput".to_string()),
        "powershell" => Ok("PowerShell".to_string()),
        "bash" => Ok("bash".to_string()),
        "skill" => Ok("Skill".to_string()),
        "agent" => Ok("Agent".to_string()),
        "sleep" => Ok("Sleep".to_string()),
        "config" => Ok("Config".to_string()),
        "repl" => Ok("REPL".to_string()),
        _ => Err(format!("unsupported tool: {raw_name}")),
    }
}

struct JsonScanner<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> JsonScanner<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn find_next_brace_block(&mut self) -> Option<(usize, usize)> {
        while let Some(start_offset) = self.text[self.pos..].find('{') {
            let absolute_start = self.pos + start_offset;
            let mut brace_level = 0;
            let mut in_string = false;
            let mut is_escaped = false;

            let mut char_indices = self.text[absolute_start..].char_indices();
            for (offset, c) in char_indices {
                if in_string {
                    if is_escaped {
                        is_escaped = false;
                    } else if c == '\\' {
                        is_escaped = true;
                    } else if c == '"' {
                        in_string = false;
                    }
                    continue;
                }

                match c {
                    '"' => {
                        in_string = true;
                        is_escaped = false;
                    }
                    '{' => brace_level += 1,
                    '}' => {
                        brace_level -= 1;
                        if brace_level == 0 {
                            let absolute_end = absolute_start + offset;
                            self.pos = absolute_end + 1;
                            return Some((absolute_start, absolute_end));
                        }
                    }
                    _ => {}
                }
            }
            // If we didn't find a matching '}', advance past this '{' and try again
            self.pos = absolute_start + 1;
        }
        None
    }
}

pub fn normalize_json_tool_calls(events: &mut Vec<StreamEvent>) {
    let mut i = 0;
    while i < events.len() {
        if let StreamEvent::ContentBlockStart(crate::types::ContentBlockStartEvent { index, content_block: crate::types::OutputContentBlock::Text { text } }) = &events[i] {
            let target_index = *index;
            let start_pos = i;
            let mut j = i + 1;
            let mut full_text = text.clone();
            let mut found_stop = false;
            let mut block_indices = vec![i];

            while j < events.len() {
                match &events[j] {
                    StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent { index: d_idx, delta: crate::types::ContentBlockDelta::TextDelta { text }, .. }) if *d_idx == target_index => {
                        full_text.push_str(text);
                        block_indices.push(j);
                    }
                    StreamEvent::ContentBlockStop(crate::types::ContentBlockStopEvent { index: s_idx }) if *s_idx == target_index => {
                        found_stop = true;
                        block_indices.push(j);
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }

            if found_stop {
                let mut new_events = Vec::new();
                let mut next_block_index = *index;
                let mut tools_found = 0;
                let mut last_narrative_pos = 0;
                
                let text_to_scan = full_text.as_str();

                // 1. Peel off <think> if present
                if let Some(start_tag) = text_to_scan.find("<think>") {
                    if let Some(end_tag) = text_to_scan.find("</think>") {
                        let total_len = end_tag + "</think>".len();
                        let thinking = &text_to_scan[start_tag..total_len];
                        push_normalized_text_block(&mut new_events, thinking, next_block_index);
                        next_block_index += 1;
                        last_narrative_pos = total_len;
                    }
                }

                let mut scanner = JsonScanner::new(text_to_scan);
                scanner.pos = last_narrative_pos;

                while let Some((start, end)) = scanner.find_next_brace_block() {
                    let candidate_json = &text_to_scan[start..=end];
                    let json_normalized = candidate_json
                        .replace(['\u{201C}', '\u{201D}'], "\"")
                        .replace(['\u{2018}', '\u{2019}'], "'");

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_normalized) {
                        if let Some(obj) = parsed.as_object() {
                            if obj.contains_key("name") && obj.contains_key("arguments") {
                                // First, emit any narrative text BEFORE this tool
                                let narrative_before = text_to_scan[last_narrative_pos..start].trim();
                                if !narrative_before.is_empty() {
                                    let mut cleaned = narrative_before;
                                    if let Some(s) = cleaned.strip_suffix("```json") { cleaned = s; }
                                    if let Some(s) = cleaned.strip_suffix("```") { cleaned = s; }
                                    cleaned = cleaned.trim();

                                    if !cleaned.is_empty() {
                                        push_normalized_text_block(&mut new_events, cleaned, next_block_index);
                                        next_block_index += 1;
                                    }
                                }

                                // Emit the Tool Use
                                let raw_tool_name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let tool_name = get_normalized_tool_name(raw_tool_name)
                                    .unwrap_or_else(|_| raw_tool_name.to_string());
                                let arguments = obj.get("arguments").unwrap().clone();

                                new_events.push(StreamEvent::ContentBlockStart(crate::types::ContentBlockStartEvent {
                                    index: next_block_index,
                                    content_block: crate::types::OutputContentBlock::ToolUse {
                                        id: format!("call_json_upcast_{}_{}", *index, tools_found),
                                        name: tool_name,
                                        input: arguments,
                                    }
                                }));
                                new_events.push(StreamEvent::ContentBlockStop(crate::types::ContentBlockStopEvent { index: next_block_index }));
                                
                                next_block_index += 1;
                                tools_found += 1;
                                last_narrative_pos = end + 1;
                            }
                        }
                    }
                }

                // Emit final narrative
                let final_narrative = text_to_scan[last_narrative_pos..].trim();
                let mut cleaned_final = final_narrative;
                if let Some(s) = cleaned_final.strip_prefix("```") { cleaned_final = s; }
                if let Some(s) = cleaned_final.strip_suffix("```") { cleaned_final = s; }
                cleaned_final = cleaned_final.trim();

                if !cleaned_final.is_empty() {
                    push_normalized_text_block(&mut new_events, cleaned_final, next_block_index);
                }

                if tools_found > 0 {
                    let new_events_count = new_events.len();
                    block_indices.sort_unstable_by(|a, b| b.cmp(a));
                    for idx in block_indices {
                        events.remove(idx);
                    }
                    for (offset, evt) in new_events.into_iter().enumerate() {
                        events.insert(start_pos + offset, evt);
                    }
                    i = start_pos + new_events_count;
                    continue;
                }
            }
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Role-based L0 dispatch + sequential escalation chain.
#[derive(Clone)]
pub struct Router {
    thinker: Box<dyn InferenceProvider>,
    typist: Box<dyn InferenceProvider>,
    escalation_chain: Vec<Box<dyn InferenceProvider>>,
    max_parse_retries: u32,
    typist_capabilities: HashSet<String>,
    context_manager: Option<crate::context::manager::ContextManager>,
}

impl Router {
    #[must_use] 
    pub fn new(
        thinker: Box<dyn InferenceProvider>,
        typist: Box<dyn InferenceProvider>,
        escalation_chain: Vec<Box<dyn InferenceProvider>>,
        max_parse_retries: u32,
        typist_capabilities: HashSet<String>,
        context_manager: Option<crate::context::manager::ContextManager>,
    ) -> Self {
        Self {
            thinker,
            typist,
            escalation_chain,
            max_parse_retries,
            typist_capabilities,
            context_manager,
        }
    }

    /// Returns the number of tiers including L0.
    #[must_use]
    pub fn tier_count(&self) -> usize {
        2 + self.escalation_chain.len() // thinker + typist + chain
    }

    /// Selects the L0 provider based on the tool names present in the request.
    ///
    /// If any tool name matches a typist capability (`bash`, `file_edit`,
    /// `python`, etc.), routes to the typist. Otherwise routes to the thinker.
    fn select_l0_provider(&self, request: &MessageRequest) -> &dyn InferenceProvider {
        if let Some(tools) = &request.tools {
            for tool in tools {
                if self.typist_capabilities.contains(&tool.name) {
                    return self.typist.as_ref();
                }
            }
        }
        self.thinker.as_ref()
    }

    /// Core routing method: L0 dispatch → retry → escalation chain.
    ///
    /// 1. Selects the L0 provider via `select_l0_provider`.
    /// 2. Attempts the request, retrying up to `max_parse_retries` on
    ///    structured parse failures.
    /// 3. If all retries fail or if the error is a context window breach,
    ///    walks the escalation chain (L1 → L2 → …) applying the same
    ///    retry logic at each tier.
    /// 4. Returns the first successful result, or the final error.
    pub async fn stream_with_escalation(
        &self,
        request: &MessageRequest,
    ) -> Result<Vec<StreamEvent>, ApiError> {
        let mut request = request.clone();
        
        if let Some(mgr) = &self.context_manager {
            request = mgr.secure_context(request).await?;
        }

        let l0 = self.select_l0_provider(&request);

        // Attempt L0
        match self.attempt_with_retries(l0, &request).await {
            Ok(events) => return Ok(events),
            Err(error) if should_escalate(&error) => {
                eprintln!(
                    "[router] L0 provider '{}' exhausted: {error}. Escalating.",
                    l0.provider_label()
                );
            }
            Err(error) => return Err(error),
        }

        // Walk the escalation chain
        for (i, provider) in self.escalation_chain.iter().enumerate() {
            match self.attempt_with_retries(provider.as_ref(), &request).await {
                Ok(events) => return Ok(events),
                Err(error) if should_escalate(&error) => {
                    eprintln!(
                        "[router] L{} provider '{}' exhausted: {error}. Escalating.",
                        i + 1,
                        provider.provider_label()
                    );
                }
                Err(error) => return Err(error),
            }
        }

        Err(ApiError::AllTiersExhausted {
            message: "all inference tiers in the escalation chain have failed".to_string(),
        })
    }

    async fn attempt_with_retries(
        &self,
        provider: &dyn InferenceProvider,
        request: &MessageRequest,
    ) -> Result<Vec<StreamEvent>, ApiError> {
        let mut last_error = None;
        for attempt in 0..=self.max_parse_retries {
            match provider.stream_inference(request).await {
                Ok(events) => return Ok(events),
                Err(error) => {
                    if attempt < self.max_parse_retries && is_parse_failure(&error) {
                        eprintln!(
                            "[router] parse failure from '{}' (attempt {}/{}): {error}",
                            provider.provider_label(),
                            attempt + 1,
                            self.max_parse_retries
                        );
                        last_error = Some(error);
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ApiError::AllTiersExhausted {
                message: format!(
                    "provider '{}' retries exhausted",
                    provider.provider_label()
                ),
            }
        }))
    }
}

// ---------------------------------------------------------------------------
// Error classification helpers
// ---------------------------------------------------------------------------

/// Returns true if the error warrants escalation to the next tier.
fn should_escalate(error: &ApiError) -> bool {
    is_parse_failure(error) || is_context_window_breach(error) || is_retryable_exhausted(error) || is_provider_refusal(error)
}

fn is_provider_refusal(error: &ApiError) -> bool {
    matches!(error, ApiError::ProviderRefusal(_))
}

fn is_parse_failure(error: &ApiError) -> bool {
    matches!(error, ApiError::InvalidSseFrame(_))
}

fn is_context_window_breach(error: &ApiError) -> bool {
    match error {
        ApiError::Api {
            error_type,
            message,
            ..
        } => {
            error_type
                .as_deref()
                .is_some_and(|t| t.contains("context_length") || t.contains("token_limit"))
                || message.as_deref().is_some_and(|m| {
                    m.contains("context length") || m.contains("maximum context")
                })
        }
        _ => false,
    }
}

fn is_retryable_exhausted(error: &ApiError) -> bool {
    matches!(error, ApiError::RetriesExhausted { .. })
}

// ---------------------------------------------------------------------------
// Factory: build Router from AgencyTopology config
// ---------------------------------------------------------------------------

/// Constructs a `Router` from the deserialized `AgencyTopology` config.
pub fn build_router_from_topology(
    topology: &AgencyTopology,
) -> Result<Router, ApiError> {
    // Find the thinker and typist entries
    let thinker_entry = topology.providers.get("L0_thinker").ok_or_else(|| {
        ApiError::Configuration("agency_topology: missing L0_thinker provider".to_string())
    })?;
    let typist_entry = topology.providers.get("L0_typist").ok_or_else(|| {
        ApiError::Configuration("agency_topology: missing L0_typist provider".to_string())
    })?;

    let thinker = build_inference_provider("L0_thinker", thinker_entry)?;
    let typist = build_inference_provider("L0_typist", typist_entry)?;

    // Collect typist capabilities
    let typist_capabilities: HashSet<String> =
        typist_entry.capabilities.iter().cloned().collect();

    // Build the escalation chain by following fallback_for links.
    // We collect providers that have `fallback_for` set, ordered by their
    // tier names (L1, L2, L3, L4...).
    let mut chain_entries: Vec<(&String, &ProviderEntry)> = topology
        .providers
        .iter()
        .filter(|(_, entry)| !entry.fallback_for.is_empty())
        .collect();
    chain_entries.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut escalation_chain: Vec<Box<dyn InferenceProvider>> = Vec::new();
    for (name, entry) in chain_entries {
        escalation_chain.push(build_inference_provider(name, entry)?);
    }

    let compactor = build_inference_provider("L0_thinker", thinker_entry)?;
    let context_manager = Some(crate::context::manager::ContextManager::new(
        compactor,
        8000, // Safe default max context before compaction trigger
        0.8,
    ));

    Ok(Router::new(
        thinker,
        typist,
        escalation_chain,
        topology.max_parse_retries,
        typist_capabilities,
        context_manager,
    ))
}

fn build_inference_provider(
    name: &str,
    entry: &ProviderEntry,
) -> Result<Box<dyn InferenceProvider>, ApiError> {
    // Both llama_cpp (local) and gemini (cloud) expose OpenAI-compatible endpoints.
    let base_url = entry
        .endpoint
        .clone()
        .unwrap_or_else(|| gemini_base_url_for_model(&entry.model));

    let api_key = if let Some(key) = &entry.api_key {
        key.clone()
    } else if let Some(env_var) = &entry.api_env_var {
        std::env::var(env_var).unwrap_or_default()
    } else {
        // Local engines typically don't need a key
        String::new()
    };

    let config = OpenAiCompatConfig {
        provider_name: "topology",
        api_key_env: "",
        base_url_env: "",
        default_base_url: "",
    };

    let client = OpenAiCompatClient::new(&api_key, config).with_base_url(&base_url);
    Ok(Box::new(OpenAiCompatInferenceProvider::new(
        client,
        name.to_string(),
        entry.model.clone(),
        entry.engine.clone(),
        entry.disable_tools.unwrap_or(false),
    )))
}

fn gemini_base_url_for_model(_model: &str) -> String {
    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
include!("router_tests.rs");
include!("normalization_tests.rs");
