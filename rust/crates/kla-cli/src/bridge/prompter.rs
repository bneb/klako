use std::io::{self, Write};
use runtime::{PermissionMode, PermissionRequest, PermissionPromptDecision, PermissionPrompter};

pub struct CliPermissionPrompter {
    current_mode: PermissionMode,
}

impl CliPermissionPrompter {
    #[must_use] 
    pub fn new(current_mode: PermissionMode) -> Self {
        Self { current_mode }
    }
}

#[async_trait::async_trait]
impl PermissionPrompter for CliPermissionPrompter {
    async fn decide(
        &mut self,
        request: &PermissionRequest,
    ) -> PermissionPromptDecision {
        println!();
        println!("Permission approval required");
        println!("  Tool             {}", request.tool_name);
        println!("  Current mode     {}", self.current_mode.as_str());
        println!("  Required mode    {}", request.required_mode.as_str());
        println!("  Input            {}", request.input);
        print!("Approve this tool call? [y/N]: ");
        let _ = io::stdout().flush();

        let res = tokio::task::spawn_blocking(|| {
            let mut response = String::new();
            io::stdin().read_line(&mut response).map(|_| response)
        }).await.expect("stdin task panicked");

        match res {
            Ok(response) => {
                let normalized = response.trim().to_ascii_lowercase();
                if matches!(normalized.as_str(), "y" | "yes") {
                    PermissionPromptDecision::Allow
                } else {
                    PermissionPromptDecision::Deny {
                        reason: format!(
                            "tool '{}' denied by user approval prompt",
                            request.tool_name
                        ),
                    }
                }
            }
            Err(error) => PermissionPromptDecision::Deny {
                reason: format!("permission approval failed: {error}"),
            },
        }
    }
}
