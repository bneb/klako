use crate::repl::LiveCli;
use crate::repl::prompter::WebPermissionPrompter;
use runtime::PermissionMode;

pub async fn run_notebook_loop(
    model: String,
    allowed_tools: Option<crate::AllowedToolSet>,
    permission_mode: PermissionMode,
    tx: tokio::sync::broadcast::Sender<String>,
    mut ui_input_rx: tokio::sync::mpsc::Receiver<String>,
    mut permission_rx: tokio::sync::mpsc::Receiver<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode, Some(tx.clone())).await?;
    tools::set_telemetry_sink(tx.clone());
    println!("Ⓚ Klako · ready");
    cli.print_status();

    while let Some(prompt) = ui_input_rx.recv().await {
        println!("[Notebook Engine] Detailed log: Processing UI Input -> {prompt}");
        let mut web_prompter = WebPermissionPrompter::new(permission_mode, tx.clone(), &mut permission_rx);
        if let Err(e) = cli.run_turn_with_prompter(&prompt, Some(&mut web_prompter)).await {
            println!("[System Fail-Safe]\n**Execution Interrupted:** Cannot complete sequence. `{e}`");
            let _ = tx.send(serde_json::json!({
                "type": "CanvasTelemetry",
                "line": format!("[Notebook Execution Error] {}", e)
            }).to_string());
            
            // Send the error as a narrative delta so it shows up in the main chat view
            let _ = tx.send(serde_json::json!({
                "type": "NarrativeDelta",
                "role": "thinker",
                "tier": "System",
                "text": format!("\n\n> **Execution Interrupted:** Cannot complete sequence. `{}`", e)
            }).to_string());
            
            let _ = tx.send(serde_json::json!({
                "type": "StatusUpdate",
                "role": "idle",
                "tier": "Error // Idle"
            }).to_string());
        } else {
            let _ = tx.send(serde_json::json!({
                "type": "StatusUpdate",
                "role": "idle",
                "tier": "Idle"
            }).to_string());
        }
    }
    Ok(())
}
