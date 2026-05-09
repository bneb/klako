use crate::ApiClient;
use crate::ApiRequest;
use crate::AssistantEvent;
use crate::session::ConversationMessage;
use serde::{Deserialize, Serialize};

pub struct AxiomValidator {
    provider: Box<dyn ApiClient>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AxiomValidationResult {
    pub passed: bool,
    pub reasoning: String,
}

impl AxiomValidator {
    #[must_use] 
    pub fn new(provider: Box<dyn ApiClient>) -> Self {
        Self { provider }
    }

    pub async fn validate(&mut self, axioms: &str, changes: &str) -> Result<AxiomValidationResult, String> {
        let system_prompt = vec![
            "You are a strict Axiom Validator. Your job is to audit a set of code changes against a set of project axioms (rules).".to_string(),
            "Output ONLY a JSON object like {\"passed\": true, \"reasoning\": \"...\"} or {\"passed\": false, \"reasoning\": \"...\"}.".to_string(),
        ];
        
        let prompt = format!(
            "### Project Axioms:\n{axioms}\n\n### Code Changes:\n{changes}\n\nPlease validate the changes against the axioms."
        );

        let request = ApiRequest {
            system_prompt,
            messages: vec![ConversationMessage::user_text(prompt)],
        };

        let events = self.provider.stream(request).await.map_err(|e| e.to_string())?;
        let mut full_text = String::new();
        for event in events {
            if let AssistantEvent::TextDelta(text) = event {
                full_text.push_str(&text);
            }
        }

        if let Some(json_start) = full_text.find('{') {
            if let Some(json_end) = full_text.rfind('}') {
                let json_str = &full_text[json_start..=json_end];
                return serde_json::from_str(json_str).map_err(|e| e.to_string());
            }
        }

        Err(format!("Validator failed to produce valid JSON: {full_text}"))
    }
}
