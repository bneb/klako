use serde::{Deserialize, Serialize};
use crate::types::{InputContentBlock, InputMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVector {
    pub active_directive: String,
    pub established_facts: Vec<String>,
    pub falsified_paths: Vec<String>,
    pub current_blockers: Vec<String>,
    pub immediate_next_step: String,
}

impl StateVector {
    /// Renders the JSON into an Assistant Message payload
    #[must_use] 
    pub fn into_message(self) -> InputMessage {
        let content = format!(
            "[SYSTEM DIAGNOSTIC: PREVIOUS CONTEXT COMPRESSED]\n{}",
            serde_json::to_string_pretty(&self).expect("serialization infallible")
        );
        InputMessage {
            role: "assistant".to_string(),
            content: vec![InputContentBlock::Text { text: content }],
        }
    }
}
