use api::router::build_router_from_topology;
use api::{
    InputMessage, MessageRequest, OutputContentBlock, StreamEvent, ToolDefinition,
};
use runtime::{AgencyTopology, EscalationPolicy, ProviderEntry};
use std::collections::BTreeMap;

#[tokio::test]
#[ignore = "Requires local llama_cpp instance running on port 11434"]
async fn test_local_model_tool_use_with_gbnf() {
    let mut providers = BTreeMap::new();
    
    // L0_thinker (Local)
    providers.insert(
        "L0_thinker".to_string(),
        ProviderEntry {
            engine: "llama_cpp".to_string(),
            model: "qwen2.5-coder:latest".to_string(),
            endpoint: Some("http://localhost:11434/v1".to_string()),
            api_env_var: None,
            api_key: None,
            capabilities: vec!["reasoning".to_string(), "chat".to_string(), "agent".to_string()],
            fallback_for: vec![],
            disable_tools: Some(false),
            skills: vec![],
        },
    );

    // L0_typist (Local)
    providers.insert(
        "L0_typist".to_string(),
        ProviderEntry {
            engine: "llama_cpp".to_string(),
            model: "qwen2.5-coder:latest".to_string(),
            endpoint: Some("http://localhost:11434/v1".to_string()),
            api_env_var: None,
            api_key: None,
            capabilities: vec!["bash".to_string(), "file_edit".to_string()],
            fallback_for: vec![],
            disable_tools: Some(false),
            skills: vec![],
        },
    );

    let topology = AgencyTopology {
        default_tier: "L0".to_string(),
        escalation_policy: EscalationPolicy::SequentialChain,
        max_parse_retries: 1, // Minimize retries for test
        providers,
    };

    let router = build_router_from_topology(&topology).expect("Failed to build router");

    let tools = vec![ToolDefinition {
        name: "bash".to_string(),
        description: Some("Execute bash commands".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        }),
    }];

    let request = MessageRequest {
        model: "qwen2.5-coder:latest".to_string(),
        max_tokens: 200,
        messages: vec![InputMessage::user_text(
            "Execute the bash command 'echo local_escalation_gbnf_test'",
        )],
        system: Some("You are a helpful assistant. You must use tools when requested.".to_string()),
        tools: Some(tools),
        tool_choice: None,
        force_json_schema: None, // Will be auto-injected by router for local model
        stream: false,
    };

    let events = router.stream_with_escalation(&request).await.expect("Router failed");

    println!("Received Events: {:#?}", events);

    let mut found_tool_use = false;
    for event in events {
        if let StreamEvent::ContentBlockStart(start) = event {
            if let OutputContentBlock::ToolUse { name, .. } = start.content_block {
                if name == "bash" {
                    found_tool_use = true;
                }
            }
        }
    }

    assert!(
        found_tool_use,
        "Local model did not produce a structurally valid tool call. GBNF enforcement may be failing."
    );
}
