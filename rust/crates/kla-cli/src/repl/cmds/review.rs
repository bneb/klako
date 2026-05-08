use crate::repl::LiveCli;

impl LiveCli {
    pub(crate) fn run_review(&self, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = path.unwrap_or("docs/design/latest.md");
        println!("Opening design document for review: {}", file_path);
        
        let content = std::fs::read_to_string(file_path).unwrap_or_else(|e| format!("Error reading file: {}", e));
        
        // Broadcast to the Notebook UI to open the review pane with this file and its content
        if let Some(tx) = &self.tx {
            let _ = tx.send(serde_json::json!({
                "type": "OpenReviewPane",
                "file_path": file_path,
                "content": content
            }).to_string());
        }
        
        println!("Please switch to the Notebook UI to interactively review and discuss the document.");
        Ok(())
    }
}