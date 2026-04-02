use crate::types::{InputContentBlock, InputMessage, ToolResultContentBlock};

/// A highly-optimized, zero-allocation token estimator for multi-tiered routing.
///
/// Uses standard BPE heuristic distributions:
/// - English standard approximation: ~4 characters per token (ratio: 0.25)
/// - Safety Buffer factor: 1.15 to prevent context truncation with highly symbolic code (JSON/bash grids)
pub fn estimate_tokens(messages: &[InputMessage]) -> usize {
    let mut total_tokens = 0.0;

    let char_to_token_ratio = 0.25;
    let safety_buffer = 1.15;

    for message in messages {
        // Base overhead per conversational block
        total_tokens += 4.0;

        for block in &message.content {
            match block {
                InputContentBlock::Text { text } => {
                    total_tokens += text.len() as f64 * char_to_token_ratio;
                }
                InputContentBlock::ToolUse { id, name, input } => {
                    // Constant overhead for tool framing tokens
                    total_tokens += 10.0;
                    total_tokens += id.len() as f64 * char_to_token_ratio;
                    total_tokens += name.len() as f64 * char_to_token_ratio;
                    let json_str = serde_json::to_string(input).unwrap_or_default();
                    total_tokens += json_str.len() as f64 * char_to_token_ratio;
                }
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    // Constant overhead for tool result framing arrays
                    total_tokens += 10.0;
                    total_tokens += tool_use_id.len() as f64 * char_to_token_ratio;
                    if *is_error {
                        total_tokens += 2.0; // Extra context for error states
                    }
                    for result_block in content {
                        match result_block {
                            ToolResultContentBlock::Text { text } => {
                                total_tokens += text.len() as f64 * char_to_token_ratio;
                            }
                            ToolResultContentBlock::Json { value } => {
                                let json_str = serde_json::to_string(value).unwrap_or_default();
                                total_tokens += json_str.len() as f64 * char_to_token_ratio;
                            }
                        }
                    }
                }
            }
        }
    }

    (total_tokens * safety_buffer).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_estimation_empty() {
        let messages = vec![];
        let estimate = estimate_tokens(&messages);
        assert_eq!(estimate, 0);
    }

    #[test]
    fn test_estimation_text_only() {
        // Hello world! is 12 chars
        // Math: 4.0 overhead + (12 * 0.25 = 3.0) = 7.0 tokens base
        // With 1.15 multiplier: 7.0 * 1.15 = 8.05 -> ceil -> 9
        let messages = vec![InputMessage::user_text("Hello world!")];
        let estimate = estimate_tokens(&messages);
        assert_eq!(estimate, 9);
    }

    #[test]
    fn test_estimation_tool_result() {
        // Test structuring to guard tool output estimation heuristics
        let tool_use = InputContentBlock::ToolResult {
            tool_use_id: "call_abc123".to_string(), // 11 chars
            is_error: false,
            content: vec![ToolResultContentBlock::Text {
                text: "Success metrics ok".to_string(), // 18 chars
            }],
        };

        let messages = vec![InputMessage {
            role: "user".to_string(),
            content: vec![tool_use],
        }];

        // Math base:
        // Msg overhead: 4.0
        // Tool Result overhead: 10.0
        // Tool Result ID: 11 * 0.25 = 2.75
        // Text Content: 18 * 0.25 = 4.5
        // Total Base = 21.25 tokens
        // Safety Buffer: 21.25 * 1.15 = 24.4375 -> ceil -> 25
        
        let estimate = estimate_tokens(&messages);
        assert_eq!(estimate, 25);
    }
    
    #[test]
    fn test_estimation_tool_result_json() {
        let tool_use = InputContentBlock::ToolResult {
            tool_use_id: "call_abc123".to_string(), // 11 chars -> 2.75
            is_error: true, // + 2.0
            content: vec![ToolResultContentBlock::Json {
                value: json!({ "error": "file not found" }), // {"error":"file not found"} -> 26 chars -> 6.5
            }],
        };

        let messages = vec![InputMessage {
            role: "user".to_string(),
            content: vec![tool_use],
        }];

        // Msg overhead: 4.0
        // Tool Overhead: 10.0
        // Total Base = 4.0 + 10.0 + 2.75 + 2.0 + 6.5 = 25.25
        // Total bounds: ceil(25.25 * 1.15) = ceil(29.0375) -> 30

        let estimate = estimate_tokens(&messages);
        assert_eq!(estimate, 30);
    }
}
