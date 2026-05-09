#[cfg(test)]
mod normalization_tests {
    use super::*;
    use crate::types::{OutputContentBlock, ContentBlockDelta, ContentBlockStartEvent, ContentBlockDeltaEvent, ContentBlockStopEvent};

    #[test]
    fn test_extracts_multiple_tool_calls_with_narrative() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "Thinking... ".to_string() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "{\"name\": \"WebSearch\", \"arguments\": {\"query\": \"test\"}} middle text {\"name\": \"write_file\", \"arguments\": {\"path\": \"foo.rs\"}}".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];

        normalize_json_tool_calls(&mut events);
        
        let tool_names: Vec<_> = events.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockStart(start) = e {
                if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                    return Some(name.clone());
                }
            }
            None
        }).collect();

        assert_eq!(tool_names, vec!["WebSearch", "write_file"]);
    }

    #[test]
    fn test_handles_nested_braces_in_json() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: String::new() }
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: "{\"name\": \"complex\", \"arguments\": {\"deep\": {\"key\": \"value\"}}}".to_string() }
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 }),
        ];

        normalize_json_tool_calls(&mut events);
        
        if let StreamEvent::ContentBlockStart(start) = &events[0] {
            if let OutputContentBlock::ToolUse { name, input, .. } = &start.content_block {
                assert_eq!(name, "complex");
                assert_eq!(input["deep"]["key"], "value");
            } else {
                panic!("Expected ToolUse");
            }
        }
    }
}
