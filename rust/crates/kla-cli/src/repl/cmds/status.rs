use crate::repl::LiveCli;
use crate::reporting::{status_context, format_status_report};

impl LiveCli {
    pub fn print_status(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        let latest = self.runtime.usage().current_turn_usage();
        let status_block = format_status_report(
            &self.model,
            crate::StatusUsage {
                message_count: self.runtime.session().messages.len(),
                turns: self.runtime.usage().turns(),
                latest,
                cumulative,
                estimated_tokens: self.runtime.estimated_tokens(),
            },
            self.permission_mode.as_str(),
            &status_context(Some(&self.session.path)).expect("status context should load"),
        );
        println!("{status_block}");
        if let Some(tx) = &self.tx {
            for line in status_block.lines() {
                let payload = serde_json::json!({
                    "type": "CanvasTelemetry",
                    "line": line
                });
                let _ = tx.send(payload.to_string());
            }
        }
    }
}
