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
pub trait InferenceProvider: Send + Sync {
    fn stream_inference<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>>;

    fn provider_label(&self) -> &str;
}

// ---------------------------------------------------------------------------
// OpenAiCompatInferenceProvider adapter
// ---------------------------------------------------------------------------

/// Wraps the existing `OpenAiCompatClient` to implement `InferenceProvider`.
///
/// Powers both local llama_cpp endpoints (which expose OpenAI-compatible
/// `/v1/chat/completions`) and cloud Gemini endpoints (which also use
/// the OpenAI-compat surface).
pub struct OpenAiCompatInferenceProvider {
    client: OpenAiCompatClient,
    label: String,
    model: String,
}

impl OpenAiCompatInferenceProvider {
    pub fn new(client: OpenAiCompatClient, label: impl Into<String>, model: String) -> Self {
        Self {
            client,
            label: label.into(),
            model,
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
                    _ => {} // Interleaved events for other blocks or global events
                }
                j += 1;
            }

            if found_stop {
                let mut current_pos = 0;
                let mut last_narrative_pos = 0;
                let mut new_events = Vec::new();
                let mut next_block_index = *index;
                let mut tools_found = 0;
                
                let text_to_scan = full_text.as_str();

                // 1. Peel off <think> if present
                if let Some(start_tag) = text_to_scan.find("<think>") {
                    if let Some(end_tag) = text_to_scan.find("</think>") {
                        let total_len = end_tag + "</think>".len();
                        let thinking = &text_to_scan[start_tag..total_len];
                        push_normalized_text_block(&mut new_events, thinking, next_block_index);
                        next_block_index += 1;
                        current_pos = total_len;
                        last_narrative_pos = total_len;
                    } else {
                        // Partial think? Take it all for now
                        push_normalized_text_block(&mut new_events, text_to_scan, next_block_index);
                        current_pos = text_to_scan.len();
                        last_narrative_pos = current_pos;
                    }
                }

                // 2. Iteratively scan for JSON-like blocks
                while current_pos < text_to_scan.len() {
                    let remaining = &text_to_scan[current_pos..];
                    if let Some(brace_start) = remaining.find('{') {
                        let absolute_start = current_pos + brace_start;
                        
                        // Balanced brace search
                        let mut brace_level = 0;
                        let mut absolute_end = None;
                        
                        // We need a char iterator to handle multi-byte chars correctly
                        let mut char_indices = text_to_scan[absolute_start..].char_indices();
                        while let Some((offset, c)) = char_indices.next() {
                            if c == '{' {
                                brace_level += 1;
                            } else if c == '}' {
                                brace_level -= 1;
                                if brace_level == 0 {
                                    absolute_end = Some(absolute_start + offset);
                                    break;
                                }
                            }
                        }

                        if let Some(end_idx) = absolute_end {
                            let candidate_json = &text_to_scan[absolute_start..=end_idx];
                            
                            // Normalize smart quotes
                            let json_normalized = candidate_json
                                .replace('\u{201C}', "\"")
                                .replace('\u{201D}', "\"")
                                .replace('\u{2018}', "'")
                                .replace('\u{2019}', "'");

                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_normalized) {
                                if let Some(obj) = parsed.as_object() {
                                    if obj.contains_key("name") && obj.contains_key("arguments") {
                                        // Found a tool call!
                                        
                                        // First, emit any narrative text BEFORE this tool
                                        let narrative_before = text_to_scan[last_narrative_pos..absolute_start].trim();
                                        if !narrative_before.is_empty() {
                                            // Optional: Strip markdown fences if they are alone at the end
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
                                        
                                        // Advance cursors
                                        current_pos = end_idx + 1;
                                        last_narrative_pos = current_pos;
                                        continue;
                                    }
                                }
                            }
                            // Not a tool? Advance past '{' and keep looking
                            current_pos = absolute_start + 1;
                        } else {
                            // Unmatched '{'
                            current_pos = absolute_start + 1;
                        }
                    } else {
                        // No more '{'
                        break;
                    }
                }

                // 3. Emit final narrative
                let final_narrative = text_to_scan[last_narrative_pos..].trim();
                // Strip trailing markdown fence if present
                let mut cleaned_final = final_narrative;
                if let Some(s) = cleaned_final.strip_prefix("```") { cleaned_final = s; }
                if let Some(s) = cleaned_final.strip_suffix("```") { cleaned_final = s; }
                cleaned_final = cleaned_final.trim();

                if !cleaned_final.is_empty() {
                    push_normalized_text_block(&mut new_events, cleaned_final, next_block_index);
                }

                if tools_found > 0 {
                    let new_events_count = new_events.len();
                    // println!("DEBUG: tools_found={}, new_events_count={}", tools_found, new_events_count);
                    
                    // Remove original events
                    block_indices.sort_unstable_by(|a, b| b.cmp(a));
                    for idx in block_indices {
                        events.remove(idx);
                    }
                    
                    // Insert new events
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
pub struct Router {
    thinker: Box<dyn InferenceProvider>,
    typist: Box<dyn InferenceProvider>,
    escalation_chain: Vec<Box<dyn InferenceProvider>>,
    max_parse_retries: u32,
    typist_capabilities: HashSet<String>,
    context_manager: Option<crate::context::manager::ContextManager>,
}

impl Router {
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
        .filter(|(name, _)| *name != "L0_thinker" && *name != "L0_typist")
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
    )))
}

fn gemini_base_url_for_model(_model: &str) -> String {
    "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use crate::types::{StreamEvent, ContentBlockStartEvent, ContentBlockDeltaEvent, ContentBlockStopEvent, OutputContentBlock, ContentBlockDelta, MessageDeltaEvent, MessageDelta, Usage};

    #[test]
    fn test_get_normalized_tool_name() {
        // Supported variations
        assert_eq!(get_normalized_tool_name("WriteFile").unwrap(), "write_file");
        assert_eq!(get_normalized_tool_name("write_file").unwrap(), "write_file");
        assert_eq!(get_normalized_tool_name("WRITE-FILE").unwrap(), "write_file");
        assert_eq!(get_normalized_tool_name("WebSearch").unwrap(), "WebSearch");
        assert_eq!(get_normalized_tool_name("web_search").unwrap(), "WebSearch");
        assert_eq!(get_normalized_tool_name("Skill").unwrap(), "Skill");

        // Unsupported cases return error
        assert!(get_normalized_tool_name("UnknownTool123").is_err());
        assert_eq!(
            get_normalized_tool_name("NotATool").unwrap_err(),
            "unsupported tool: NotATool"
        );
    }

    struct MockProvider {
        label: String,
        fail_count: std::sync::atomic::AtomicU32,
        max_failures: u32,
    }

    impl MockProvider {
        fn always_succeed(label: &str) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::atomic::AtomicU32::new(0),
                max_failures: 0,
            }
        }

        fn fail_n_then_succeed(label: &str, n: u32) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::atomic::AtomicU32::new(0),
                max_failures: n,
            }
        }

        fn always_fail(label: &str) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::atomic::AtomicU32::new(0),
                max_failures: u32::MAX,
            }
        }
    }

    struct MockRefusingProvider {
        label: String,
        refusal_message: String,
    }

    impl InferenceProvider for MockRefusingProvider {
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>>
        {
            let refusal = self.refusal_message.clone();
            Box::pin(async move { Err(ApiError::ProviderRefusal(refusal)) })
        }

        fn provider_label(&self) -> &str {
            &self.label
        }
    }

    impl InferenceProvider for MockProvider {
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>>
        {
            let current = self
                .fail_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if current < self.max_failures {
                Box::pin(async move {
                    Err(ApiError::InvalidSseFrame("mock parse failure"))
                })
            } else {
                Box::pin(async move { Ok(Vec::new()) })
            }
        }

        fn provider_label(&self) -> &str {
            &self.label
        }
    }

    fn empty_request() -> MessageRequest {
        MessageRequest {
            model: "test".to_string(),
            max_tokens: 100,
            messages: Vec::new(),
            system: None,
            tools: None,
            tool_choice: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn routes_to_thinker_by_default() {
        let router = Router::new(
            Box::new(MockProvider::always_succeed("thinker")),
            Box::new(MockProvider::always_fail("typist")),
            Vec::new(),
            2,
            HashSet::from(["bash".to_string()]),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn routes_to_typist_for_bash_tool() {
        use crate::types::ToolDefinition;
        let router = Router::new(
            Box::new(MockProvider::always_fail("thinker")),
            Box::new(MockProvider::always_succeed("typist")),
            Vec::new(),
            2,
            HashSet::from(["bash".to_string()]),
            None,
        );
        let mut request = empty_request();
        request.tools = Some(vec![ToolDefinition {
            name: "bash".to_string(),
            description: Some("run shell".to_string()),
            input_schema: serde_json::json!({}),
        }]);
        let result = router.stream_with_escalation(&request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn escalates_to_l1_on_parse_failure() {
        let router = Router::new(
            Box::new(MockProvider::always_fail("thinker")),
            Box::new(MockProvider::always_fail("typist")),
            vec![Box::new(MockProvider::always_succeed("L1"))],
            2,
            HashSet::new(),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn escalates_through_full_chain() {
        let router = Router::new(
            Box::new(MockProvider::always_fail("thinker")),
            Box::new(MockProvider::always_fail("typist")),
            vec![
                Box::new(MockProvider::always_fail("L1")),
                Box::new(MockProvider::always_fail("L2")),
                Box::new(MockProvider::always_succeed("L3")),
            ],
            0,
            HashSet::new(),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn returns_error_when_all_tiers_fail() {
        let router = Router::new(
            Box::new(MockProvider::always_fail("thinker")),
            Box::new(MockProvider::always_fail("typist")),
            vec![Box::new(MockProvider::always_fail("L1"))],
            0,
            HashSet::new(),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn retries_before_escalating() {
        // thinker fails 2 times then succeeds on 3rd (max_parse_retries=2 gives 3 attempts)
        let router = Router::new(
            Box::new(MockProvider::fail_n_then_succeed("thinker", 2)),
            Box::new(MockProvider::always_fail("typist")),
            Vec::new(),
            2,
            HashSet::new(),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn escalates_on_provider_refusal() {
        let router = Router::new(
            Box::new(MockRefusingProvider {
                label: "thinker".to_string(),
                refusal_message: "I am an AI and cannot write a game".to_string(),
            }),
            Box::new(MockProvider::always_fail("typist")),
            vec![Box::new(MockProvider::always_succeed("L1"))],
            2,
            HashSet::new(),
            None,
        );
        let result = router
            .stream_with_escalation(&empty_request())
            .await;
        // It should escalate successfully to L1 and return Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_provider_refusal() {
        use crate::types::{ContentBlockStartEvent, OutputContentBlock, ContentBlockDeltaEvent, ContentBlockDelta};
        
        let events_safe_text = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent { index: 0, content_block: OutputContentBlock::Text { text: "Hello! I can help.".to_string() } }),
        ];
        assert_eq!(detect_provider_refusal(&events_safe_text), None);

        let events_refusal = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent { index: 0, content_block: OutputContentBlock::Text { text: "I'm sorry, I cannot fulfill".to_string() } }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent { index: 0, delta: ContentBlockDelta::TextDelta { text: " this request".to_string() } }),
        ];
        assert_eq!(detect_provider_refusal(&events_refusal).unwrap(), "I'm sorry, I cannot fulfill this request");

        let events_refusal_with_tool = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent { index: 0, content_block: OutputContentBlock::Text { text: "I'm sorry".to_string() } }),
            StreamEvent::ContentBlockStart(ContentBlockStartEvent { index: 1, content_block: OutputContentBlock::ToolUse { id: "1".into(), name: "test".into(), input: serde_json::json!({}) } }),
        ];
        // Must bypass refusal match because a tool call is present
        assert_eq!(detect_provider_refusal(&events_refusal_with_tool), None);
    }

    #[test]
    fn builds_router_from_valid_topology() {
        let mut providers = BTreeMap::new();
        providers.insert(
            "L0_thinker".to_string(),
            ProviderEntry {
                engine: "llama_cpp".to_string(),
                model: "gemma-4-E4B-it.gguf".to_string(),
                endpoint: Some("http://localhost:8080/v1".to_string()),
                api_env_var: None,
                api_key: None,
                capabilities: vec!["reasoning".to_string(), "chat".to_string()],
                fallback_for: Vec::new(),
            },
        );
        providers.insert(
            "L0_typist".to_string(),
            ProviderEntry {
                engine: "llama_cpp".to_string(),
                model: "qwen2.5-coder-7b.gguf".to_string(),
                endpoint: Some("http://localhost:8081/v1".to_string()),
                api_env_var: None,
                api_key: None,
                capabilities: vec!["bash".to_string(), "file_edit".to_string()],
                fallback_for: Vec::new(),
            },
        );
        providers.insert(
            "L1_micro".to_string(),
            ProviderEntry {
                engine: "gemini".into(),
                model: "gemini-3.1-flash-lite-preview".into(),
                endpoint: None,
                api_env_var: None,
                api_key: None,
                capabilities: Vec::new(),
                fallback_for: vec!["L0_thinker".to_string(), "L0_typist".to_string()],
            },
        );

        let topology = AgencyTopology {
            default_tier: "L0".to_string(),
            escalation_policy: runtime::EscalationPolicy::SequentialChain,
            max_parse_retries: 2,
            providers,
        };

        let router = build_router_from_topology(&topology);
        assert!(router.is_ok());
        let router = router.unwrap();
        assert_eq!(router.tier_count(), 3); // thinker + typist + L1
    }

    #[tokio::test]
    async fn provider_overrides_request_model() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{}", port);
        
        let client = OpenAiCompatClient::new(
            "key", 
            OpenAiCompatConfig { 
                provider_name: "test", 
                api_key_env: "", 
                base_url_env: "", 
                default_base_url: "" 
            }
        ).with_base_url(&base_url);
        
        let provider = OpenAiCompatInferenceProvider::new(client, "L0", "deepseek-r1".to_string());
        
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::AsyncReadExt;
            let mut buf = [0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            
            // Assert the topological model override is properly serialized, ignoring the generalized request model.
            assert!(req.contains("\"model\":\"deepseek-r1\""));
            assert!(!req.contains("gemini-2.5-flash"));
            
            use tokio::io::AsyncWriteExt;
            // Send back empty SSE to clear out the evaluation stream loop naturally.
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\n\r\ndata: [DONE]\n\n").await;
        });
        
        let mut request = empty_request();
        request.model = "gemini-2.5-flash".to_string();
        
        let _ = provider.stream_inference(&request).await;
    }

    #[test]
    fn normalize_json_tool_call_markdown() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "```json\n".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "{\"name\": \"WebSearch\", \"arguments\": \"{\\\"query\\\":\\\"tron\\\"}\"}\n".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "```".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // Expected blocks: 1 (ToolUse only)
        // Total 2 events.
        assert_eq!(events.len(), 2);
        if let StreamEvent::ContentBlockStart(start) = &events[0] {
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            } else { panic!("Not a ToolUse block, got {:?}", start.content_block); }
        } else { panic!("Not a ContentBlockStart"); }
    }

    #[test]
    fn normalize_json_tool_call_raw() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "{\"name\": \"WebSearch\", \"arguments\": {\"query\": \"tron\"}}".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // 1 ToolUse = 2 events
        assert_eq!(events.len(), 2);
        if let StreamEvent::ContentBlockStart(start) = &events[0] {
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            } else { panic!("Not a ToolUse block: {:?}", start.content_block); }
        }
    }

    #[test]
    fn normalize_json_tool_call_deepseek_think() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "<think>\nThinking about Tron...\n</think>\n\n".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "{\"name\": \"WebSearch\", \"arguments\": {\"query\": \"tron\"}}".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // Expect 2 content blocks: Text (thinking) + ToolUse
        // index 0: Text start, Text delta (think), Text stop
        // index 1: ToolUse start, ToolUse stop
        assert_eq!(events.len(), 5);
        
        if let StreamEvent::ContentBlockDelta(delta) = &events[1] {
            if let ContentBlockDelta::TextDelta { text } = &delta.delta {
                assert!(text.contains("<think>"));
            }
        }
        
        if let StreamEvent::ContentBlockStart(start) = &events[3] {
            assert_eq!(start.index, 1);
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            }
        }
    }
    #[test]
    fn normalize_json_tool_call_with_interleaved_delta() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "{\"name\": \"WebSearch\", \"arguments\": {\"query\": \"tron\"}}".to_string() }
            }),
            StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta { stop_reason: None, stop_sequence: None },
                usage: Usage { input_tokens: 10, cache_creation_input_tokens: 0, cache_read_input_tokens: 0, output_tokens: 20 }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // ToolUse start, Stop, AND the MessageDelta which should be kept!
        // So 3 events total.
        assert_eq!(events.len(), 3);
        
        if let StreamEvent::ContentBlockStart(start) = &events[0] {
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            } else { panic!("Not a ToolUse block, got {:?}", start.content_block); }
        }
    }

    #[test]
    fn normalize_json_tool_call_with_extra_text() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "Sure! I will search for that: {\"name\": \"WebSearch\", \"arguments\": {\"query\": \"tron\"}} Hope this helps!".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // Block 0: Text ("Sure! I will search for that:") (3)
        // Block 1: ToolUse (2)
        // Block 2: Text ("Hope this helps!") (3)
        // Total 8 events.
        assert_eq!(events.len(), 8);
        
        if let StreamEvent::ContentBlockStart(start) = &events[3] {
            assert_eq!(start.index, 1);
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            } else { panic!("Expected ToolUse at index 1, got {:?}", start.content_block); }
        }
    }

    #[test]
    fn normalize_json_tool_call_multi() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "Here is your plan:\n\n```json\n{\"name\": \"WebSearch\", \"arguments\": {\"query\": \"Cruis'n USA\"}}\n```\n\nAnd then:\n\n```json\n{\"name\": \"NotebookEdit\", \"arguments\": {\"notebook_path\": \"test.ipynb\", \"edit_mode\": \"replace\", \"cell_id\": \"1\", \"new_source\": \"test\"}}\n```\n\nGood luck!".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];
        
        normalize_json_tool_calls(&mut events);
        
        // Expected blocks:
        // 0: Text ("Here is your plan:") (3)
        // 1: ToolUse (WebSearch) (2)
        // 2: Text ("And then:") (3)
        // 3: ToolUse (NotebookEdit) (2)
        // 4: Text ("Good luck!") (3)
        // Total 13 events.
        assert_eq!(events.len(), 13);
        
        if let StreamEvent::ContentBlockStart(start) = &events[3] {
            assert_eq!(start.index, 1);
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "WebSearch");
            }
        }
        
        if let StreamEvent::ContentBlockStart(start) = &events[8] {
            assert_eq!(start.index, 3);
            if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                assert_eq!(name, "NotebookEdit");
            }
        }
    }
}
