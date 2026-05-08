use crate::repl::LiveCli;

impl LiveCli {
    pub(crate) fn run_rewind(&self, task_index: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
        let Some(index) = task_index else {
            println!("Usage: /rewind <task_index>");
            return Ok(());
        };

        println!("Rewinding swarm to task {}...", index);
        
        tokio::runtime::Runtime::new()?.block_on(async {
            if let Ok(cp) = runtime::workspace::checkpoint::WorkspaceCheckpoint::new(".").await {
                
                // Read the exact serialized Swarm State
                if let Ok(state_json) = std::fs::read_to_string(".kla/sessions/SWARM_STATE.json") {
                    if let Ok(mut state) = serde_json::from_str::<serde_json::Value>(&state_json) {
                        let tasks_len = state.get("tasks").and_then(|t| t.as_array()).map(|a| a.len()).unwrap_or(0);
                        
                        if tasks_len > 0 {
                            if index >= tasks_len {
                                println!("Task index out of bounds. There are only {} tasks.", tasks_len);
                                return;
                            }
                            
                            // Calculate how many tasks have been completed AFTER or ON the target index
                            let mut commits_to_rewind = 0;
                            if let Some(tasks_arr) = state.get("tasks").and_then(|t| t.as_array()) {
                                for i in index..tasks_arr.len() {
                                    if let Some(status) = tasks_arr[i].get("status").and_then(|s| s.as_str()) {
                                        if status == "Completed" {
                                            commits_to_rewind += 1;
                                        }
                                    }
                                }
                            }
                            
                            // Revert the workspace using the precise number of Auto-Checkpoint commits
                            if commits_to_rewind > 0 {
                                println!("Restoring workspace (Rewinding {} snapshots)...", commits_to_rewind);
                                if let Err(e) = cp.restore_commits(commits_to_rewind).await {
                                    println!("Warning: Failed to restore workspace tracking: {}", e);
                                }
                            }
                            
                            // Update the state machine: Mark all tasks >= index as Pending
                            if let Some(tasks_arr) = state.get_mut("tasks").and_then(|t| t.as_array_mut()) {
                                for i in index..tasks_arr.len() {
                                    tasks_arr[i]["status"] = serde_json::json!("Pending");
                                }
                            }
                            
                            state["status"] = serde_json::json!("Planning");
                            
                            // Save the updated state back to disk
                            let _ = std::fs::write(".kla/sessions/SWARM_STATE.json", serde_json::to_string_pretty(&state).unwrap());
                            
                            // Also rewrite PLAN.md so the user sees the updated plan
                            let objective_desc = state.get("objective").and_then(|o| o.get("description")).and_then(|d| d.as_str()).unwrap_or("");
                            let mut plan_content = format!("# Swarm Execution Plan\n\n**Objective:** {}\n\n*Edit the tasks below. Each line starting with `- ` is a task. Save the file and approve in the UI/CLI to begin execution.*\n\n", objective_desc);
                            
                            if let Some(tasks_arr) = state.get("tasks").and_then(|t| t.as_array()) {
                                for task in tasks_arr.iter() {
                                    if let Some(desc) = task.get("description").and_then(|d| d.as_str()) {
                                        plan_content.push_str(&format!("- {}\n", desc));
                                    }
                                }
                            }
                            
                            let _ = std::fs::write(".kla/sessions/PLAN.md", plan_content);

                            println!("Workspace restored. Swarm state reverted.");
                            println!("You can edit .kla/sessions/PLAN.md to change the remaining tasks, then run /loop to resume.");
                        }
                    } else {
                        println!("Failed to parse SWARM_STATE.json");
                    }
                } else {
                    println!("Could not find active swarm state. Run /loop to start a new swarm.");
                }
            } else {
                println!("Failed to access workspace checkpoint tracker.");
            }
        });

        Ok(())
    }
}
