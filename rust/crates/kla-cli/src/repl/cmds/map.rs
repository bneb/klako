use crate::repl::LiveCli;

impl LiveCli {
    pub async fn run_map(&self, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let target_path = path.unwrap_or(".");
        let absolute_path = std::env::current_dir()?.join(target_path);
        
        println!("Generating architecture map for: {}", absolute_path.display());
        
        let payload = serde_json::json!({
            "operation": "get_repo_map",
            "dir_path": absolute_path.to_string_lossy().to_string()
        });
        
        match tools::execute_tool("DiscoveryWorld", &payload).await {
            Ok(result) => {
                // Broadcast map data to the Notebook UI
                if let Some(tx) = &self.tx {
                    let _ = tx.send(serde_json::json!({
                        "type": "MapArtifact",
                        "target_path": target_path,
                        "map_data": serde_json::from_str::<serde_json::Value>(&result).unwrap_or(serde_json::json!({}))
                    }).to_string());
                }
                println!("Architecture map generated! Please view it in the Notebook UI.");
            }
            Err(e) => {
                println!("Failed to generate map: {e}");
            }
        }
        
        Ok(())
    }
}
