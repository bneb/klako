use crate::context::estimator::estimate_tokens;
use crate::context::vector::StateVector;
use crate::error::ApiError;
use crate::router::InferenceProvider;
use crate::types::{ContentBlockDelta, InputMessage, MessageRequest, StreamEvent};

#[derive(Clone)]
pub struct ContextManager {
    compactor_provider: Box<dyn InferenceProvider>,
    max_tokens: usize,
    threshold_ratio: f64, // e.g., 0.8
}

impl ContextManager {
    #[must_use] 
    pub fn new(
        compactor_provider: Box<dyn InferenceProvider>,
        max_tokens: usize,
        threshold_ratio: f64,
    ) -> Self {
        Self {
            compactor_provider,
            max_tokens,
            threshold_ratio,
        }
    }

    /// Evaluates the request. If tokens exceed threshold, performs compaction inline.
    pub async fn secure_context(
        &self,
        mut request: MessageRequest,
    ) -> Result<MessageRequest, ApiError> {
        let current_tokens = estimate_tokens(&request.messages);
        let threshold = (self.max_tokens as f64 * self.threshold_ratio) as usize;

        if current_tokens < threshold {
            return Ok(request);
        }

        // Cannot compact if we have too few messages
        let len = request.messages.len();
        let tail_n = 4; // keep the last 4 messages (e.g., 2 user/assistant turns)
        if len <= tail_n + 1 {
            // + 1 for system prompt (or first message)
            return Ok(request);
        }

        // 1. Extract slices
        let system_msg = request.messages[0].clone();
        let middle_block = &request.messages[1..(len - tail_n)];
        let tail_block = &request.messages[(len - tail_n)..len];

        // 2. Wrap middle block into compressor request
        let middle_json = serde_json::to_string_pretty(middle_block)
            .unwrap_or_else(|_| "[]".to_string());
            
        let prompt_text = format!(
            "SYSTEM DIRECTIVE: STATE VECTOR COMPACTION\n\
            Role: You are the Epistemological Compressor for an autonomous neuro-symbolic reasoning engine.\n\
            Objective: Compress the provided history into a highly dense, deterministic \"State Vector Checkpoint\". Discard filler and step-by-step code iterations. Losslessly preserve goals, constraints, and dead-ends.\n\
            \n\
            OUTPUT SCHEMA:\n\
            Return ONLY valid JSON matching this exact structure:\n\
            {{\n\
              \"active_directive\": \"string\",\n\
              \"established_facts\": [\"string\"],\n\
              \"falsified_paths\": [\"string\"],\n\
              \"current_blockers\": [\"string\"],\n\
              \"immediate_next_step\": \"string\"\n\
            }}\n\
            \n\
            HISTORY TO COMPRESS:\n\
            {middle_json}"
        );

        let mut compactor_req = MessageRequest {
            model: self.compactor_provider.provider_label().to_string(), // use the provider's native model
            max_tokens: 1500,
            messages: vec![InputMessage::user_text(prompt_text)],
            system: None,
            tools: None,
            tool_choice: None,
            force_json_schema: None,
            stream: true,
        };

        // 3. Retry loop for LLM parsing
        let mut retries = 0;
        let mut compacted_vector: Option<StateVector> = None;

        while retries < 2 {
            let stream_result = self.compactor_provider.stream_inference(&compactor_req).await;
            if let Ok(events) = stream_result {
                let mut output_json = String::new();
                for event in events {
                    if let StreamEvent::ContentBlockDelta(evt) = event {
                        if let ContentBlockDelta::TextDelta { text } = evt.delta {
                            output_json.push_str(&text);
                        }
                    }
                }

                // Try to parse the JSON (stripping trailing/leading markdown blocks if any)
                let cleaned = output_json
                    .trim()
                    .strip_prefix("```json")
                    .unwrap_or(&output_json)
                    .strip_prefix("```")
                    .unwrap_or(&output_json)
                    .strip_suffix("```")
                    .unwrap_or(&output_json)
                    .trim();

                if let Ok(vector) = serde_json::from_str::<StateVector>(cleaned) {
                    compacted_vector = Some(vector);
                    break;
                } else {
                    // Retry with fixed reminder
                    compactor_req.messages.push(InputMessage {
                        role: "assistant".to_string(),
                        content: vec![crate::types::InputContentBlock::Text {
                            text: output_json,
                        }],
                    });
                    compactor_req.messages.push(InputMessage::user_text(
                        "ERROR: Output did not match strict JSON schema. Return ONLY the JSON object.",
                    ));
                }
            }
            retries += 1;
        }

        // 4. Rebuild request.messages
        let mut new_messages = vec![system_msg];
        
        if let Some(vector) = compacted_vector {
            new_messages.push(vector.into_message());
        }
        // If compacted_vector is None, the fallback constraint triggers: 
        // We gracefully degrade to pure deterministic FIFO truncation by simply NOT inserting a StateVector, 
        // thereby dropping the oldest turns natively.
        
        new_messages.extend_from_slice(tail_block);
        
        
        request.messages = new_messages;
        
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputContentBlock, ContentBlockDeltaEvent};
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Clone)]
    struct MockProvider {
        label: String,
        return_events: Vec<StreamEvent>,
    }

    impl InferenceProvider for MockProvider {
        fn provider_label(&self) -> &str {
            &self.label
        }
        
        fn stream_inference<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<StreamEvent>, ApiError>> + Send + 'a>> {
            // Because Box::pin requires the future to be 'a + Send, and we must clone return_events:
            let events = self.return_events.clone();
            Box::pin(async move { Ok(events) })
        }
    }

    #[tokio::test]
    async fn test_secure_context_fifo_fallback() {
        // A mock provider that returns invalid jumbled text instead of JSON
        let mock_provider = Box::new(MockProvider {
            label: "L0_thinker".to_string(),
            return_events: vec![
                StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta { text: "this is not json".to_string() }
                })
            ]
        });

        let manager = ContextManager::new(mock_provider, 20, 0.5); // very small threshold to trigger it

        let mut msgs = vec![];
        for i in 0..10 {
            msgs.push(InputMessage::user_text(format!("Turn {}", i)));
        }
        let req = MessageRequest {
            model: "test".to_string(),
            max_tokens: 100,
            messages: msgs,
            system: None,
            tools: None,
            tool_choice: None,
            force_json_schema: None,
            stream: false,
        };

        let result = manager.secure_context(req).await.unwrap();
        // Fallback constraint activated: We kept System (Turn 0) and Tail 4 (Turns 6, 7, 8, 9)
        // No StateVector was successfully parsed, so it degraded purely to FIFO split logic!
        assert_eq!(result.messages.len(), 5);
        assert_eq!(result.messages[0].content, vec![InputContentBlock::Text { text: "Turn 0".to_string() }]);
        assert_eq!(result.messages[1].content, vec![InputContentBlock::Text { text: "Turn 6".to_string() }]);
    }
}
