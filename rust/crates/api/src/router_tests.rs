#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputContentBlock, MessageRequest, StreamEvent};
    use crate::error::ApiError;
    use std::pin::Pin;
    use std::future::Future;
    use std::collections::HashSet;

    #[test]
    fn test_normalized_tool_names() {
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

    #[derive(Clone)]
    struct MockProvider {
        label: String,
        fail_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        max_failures: u32,
    }

    impl MockProvider {
        fn new(label: &str, max_failures: u32) -> Self {
            Self {
                label: label.to_string(),
                fail_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                max_failures,
            }
        }

        fn fail_n_then_succeed(label: &str, n: u32) -> Self {
            Self::new(label, n)
        }

        fn always_fail(label: &str) -> Self {
            Self::new(label, u32::MAX)
        }
        
        fn always_succeed(label: &str) -> Self {
            Self::new(label, 0)
        }
    }

    impl InferenceProvider for MockProvider {
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>> {
            let label = self.label.clone();
            let fail_count = self.fail_count.clone();
            let max_failures = self.max_failures;

            Box::pin(async move {
                let current = fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if current < max_failures {
                    return Err(ApiError::ProviderRefusal(format!(
                        "provider {} failing purposefully ({} < {})",
                        label, current, max_failures
                    )));
                }

                Ok(vec![
                    StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent {
                        index: 0,
                        delta: crate::types::ContentBlockDelta::TextDelta {
                            text: format!("Success from {}", label),
                        },
                    }),
                    StreamEvent::MessageStop(crate::types::MessageStopEvent {}),
                ])
            })
        }

        fn provider_label(&self) -> &str {
            &self.label
        }
    }

    #[derive(Clone)]
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

    fn empty_request() -> MessageRequest {
        MessageRequest {
            model: "test".to_string(),
            max_tokens: 100,
            messages: Vec::new(),
            system: None,
            tools: None,
            tool_choice: None,
            force_json_schema: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn test_router_sequential_escalation() {
        let p1 = Box::new(MockProvider::always_fail("P1"));
        let p2 = Box::new(MockProvider::always_fail("P2"));
        let p3 = Box::new(MockProvider::new("P3", 0));

        let mut router = Router::new(
            p1.clone(),
            p1,
            vec![p2, p3],
            0,
            HashSet::new(),
            None,
        );

        let events = router.stream_with_escalation(&empty_request()).await.unwrap();
        let last_text = events.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockDelta(d) = e {
                if let crate::types::ContentBlockDelta::TextDelta { text } = &d.delta {
                    return Some(text.as_str());
                }
            }
            None
        }).last().unwrap();
        
        assert_eq!(last_text, "Success from P3");
    }

    #[tokio::test]
    async fn test_router_exhaustion() {
        let p1 = Box::new(MockProvider::always_fail("P1"));
        let p2 = Box::new(MockProvider::always_fail("P2"));

        let mut router = Router::new(
            p1.clone(),
            p1,
            vec![p2],
            0,
            HashSet::new(),
            None,
        );

        let res = router.stream_with_escalation(&empty_request()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_router_tool_routing() {
        let thinker = Box::new(MockProvider::new("Thinker", 0));
        let typist = Box::new(MockProvider::new("Typist", 0));
        
        let mut caps = HashSet::new();
        caps.insert("bash".to_string());

        let mut router = Router::new(
            thinker,
            typist,
            Vec::new(),
            0,
            caps,
            None,
        );

        // Thinker request
        let res1 = router.stream_with_escalation(&empty_request()).await.unwrap();
        let first_text = res1.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockDelta(d) = e {
                if let crate::types::ContentBlockDelta::TextDelta { text } = &d.delta {
                    return Some(text.as_str());
                }
            }
            None
        }).next().unwrap();
        assert!(first_text.contains("Thinker"));

        // Typist request
        let mut tool_req = empty_request();
        tool_req.tools = Some(vec![crate::types::ToolDefinition {
            name: "bash".to_string(),
            description: None,
            input_schema: serde_json::json!({}),
        }]);
        let res2 = router.stream_with_escalation(&tool_req).await.unwrap();
        let first_text2 = res2.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockDelta(d) = e {
                if let crate::types::ContentBlockDelta::TextDelta { text } = &d.delta {
                    return Some(text.as_str());
                }
            }
            None
        }).next().unwrap();
        assert!(first_text2.contains("Typist"));
    }

    #[tokio::test]
    async fn test_router_refusal_escalation() {
        let p1 = Box::new(MockRefusingProvider {
            label: "P1".to_string(),
            refusal_message: "I cannot do that".to_string(),
        });
        let p2 = Box::new(MockProvider::new("P2", 0));

        let mut router = Router::new(
            p1.clone(),
            p1,
            vec![p2],
            0,
            HashSet::new(),
            None,
        );

        let res = router.stream_with_escalation(&empty_request()).await.unwrap();
        let last_text = res.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockDelta(d) = e {
                if let crate::types::ContentBlockDelta::TextDelta { text } = &d.delta {
                    return Some(text.as_str());
                }
            }
            None
        }).last().unwrap();
        assert_eq!(last_text, "Success from P2");
    }

    #[test]
    fn test_complex_event_normalization() {
        let mut events = vec![
            StreamEvent::ContentBlockStart(crate::types::ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text { text: "Thought process... ".to_string() },
            }),
            StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent {
                index: 0,
                delta: crate::types::ContentBlockDelta::TextDelta { text: "Here is the code: ```json {\"name\": \"WebSearch\", \"arguments\": {\"query\": \"test\"}} ```".to_string() },
            }),
            StreamEvent::ContentBlockStop(crate::types::ContentBlockStopEvent { index: 0 }),
            StreamEvent::ContentBlockStart(crate::types::ContentBlockStartEvent {
                index: 1,
                content_block: OutputContentBlock::Text { text: "Now for the other one: ".to_string() },
            }),
            StreamEvent::ContentBlockDelta(crate::types::ContentBlockDeltaEvent {
                index: 1,
                delta: crate::types::ContentBlockDelta::TextDelta { text: "{\"name\": \"NotebookEdit\", \"arguments\": {\"path\": \"foo.rs\"}}".to_string() },
            }),
            StreamEvent::ContentBlockStop(crate::types::ContentBlockStopEvent { index: 1 }),
        ];

        normalize_json_tool_calls(&mut events);
        
        let tools: Vec<_> = events.iter().filter_map(|e| {
            if let StreamEvent::ContentBlockStart(start) = e {
                if let OutputContentBlock::ToolUse { name, .. } = &start.content_block {
                    return Some(name.clone());
                }
            }
            None
        }).collect();
        
        assert_eq!(tools, vec!["WebSearch", "NotebookEdit"]);
    }
}
