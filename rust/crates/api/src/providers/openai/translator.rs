use serde_json::{json, Value};
use crate::types::{
    InputContentBlock, InputMessage, MessageRequest, ToolChoice, ToolDefinition, ToolResultContentBlock,
};

pub fn build_chat_completion_request(request: &MessageRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = request.system.as_ref().filter(|value| !value.is_empty()) {
        messages.push(json!({
            "role": "system",
            "content": system,
        }));
    }
    for message in &request.messages {
        messages.extend(translate_message(message, request.model.as_str()));
    }

    let mut payload = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": request.stream,
    });

    if let Some(tools) = &request.tools {
        payload["tools"] =
            Value::Array(tools.iter().map(openai_tool_definition).collect::<Vec<_>>());
    }
    if let Some(tool_choice) = &request.tool_choice {
        payload["tool_choice"] = openai_tool_choice(tool_choice);
    }
    
    if let Some(schema) = &request.force_json_schema {
        payload["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {
                "name": "strict_gbnf_output",
                "schema": schema,
                "strict": true
            }
        });
    }

    payload
}

#[must_use] 
pub fn translate_message(message: &InputMessage, model: &str) -> Vec<Value> {
    match message.role.as_str() {
        "assistant" => {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for block in &message.content {
                match block {
                    InputContentBlock::Text { text: value } => text.push_str(value),
                    InputContentBlock::ToolUse { id, name, input } => {
                        if model.contains("gemini-3") {
                            let input_args = if input.is_null() { "{}" } else { &input.to_string() };
                            text.push_str(&format!("\n<tool_use id=\"{id}\" name=\"{name}\">\n{input_args}\n</tool_use>"));
                        } else {
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string(),
                                }
                            }));
                        }
                    }
                    InputContentBlock::ToolResult { .. } => {}
                }
            }
            if text.is_empty() && tool_calls.is_empty() {
                Vec::new()
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert("role".to_string(), json!("assistant"));
                if !text.is_empty() {
                    obj.insert("content".to_string(), json!(text));
                }
                if !tool_calls.is_empty() {
                    obj.insert("tool_calls".to_string(), json!(tool_calls));
                }
                vec![Value::Object(obj)]
            }
        }
        _ => message
            .content
            .iter()
            .filter_map(|block| match block {
                InputContentBlock::Text { text } => Some(json!({
                    "role": "user",
                    "content": text,
                })),
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    if model.contains("gemini-3") {
                        Some(json!({
                            "role": "user",
                            "content": format!("<tool_result id=\"{}\" is_error=\"{}\">\n{}\n</tool_result>", tool_use_id, is_error, flatten_tool_result_content(content)),
                        }))
                    } else {
                        Some(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": flatten_tool_result_content(content),
                            "is_error": is_error,
                        }))
                    }
                }
                InputContentBlock::ToolUse { .. } => None,
            })
            .collect(),
    }
}

fn flatten_tool_result_content(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|block| match block {
            ToolResultContentBlock::Text { text } => text.clone(),
            ToolResultContentBlock::Json { value } => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn openai_tool_definition(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn openai_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Any => Value::String("required".to_string()),
        ToolChoice::Tool { name } => json!({
            "type": "function",
            "function": { "name": name },
        }),
    }
}
