use runtime::{PermissionPrompter, PermissionRequest, PermissionPromptDecision, PermissionMode};

pub struct WebPermissionPrompter<'a> {
    tx: tokio::sync::broadcast::Sender<String>,
    permission_rx: &'a mut tokio::sync::mpsc::Receiver<String>,
}

impl<'a> WebPermissionPrompter<'a> {
    pub fn new(
        _current_mode: PermissionMode,
        tx: tokio::sync::broadcast::Sender<String>,
        permission_rx: &'a mut tokio::sync::mpsc::Receiver<String>,
    ) -> Self {
        Self {
            tx,
            permission_rx,
        }
    }
}

#[async_trait::async_trait]
impl PermissionPrompter for WebPermissionPrompter<'_> {
    async fn decide(
        &mut self,
        request: &PermissionRequest,
    ) -> PermissionPromptDecision {
        let payload = serde_json::json!({
            "type": "PermissionRequest",
            "tool": request.tool_name,
            "required_mode": request.required_mode.as_str(),
            "input": request.input
        });
        let _ = self.tx.send(payload.to_string());

        let mut decision = "deny".to_string();
        while let Some(msg) = self.permission_rx.recv().await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                if let Some(d) = parsed.get("decision").and_then(|v| v.as_str()) {
                    decision = d.to_string();
                    break;
                }
            }
        }

        if decision == "allow" {
            PermissionPromptDecision::Allow
        } else {
            PermissionPromptDecision::Deny {
                reason: format!("tool '{}' denied by web user", request.tool_name),
            }
        }
    }
}
