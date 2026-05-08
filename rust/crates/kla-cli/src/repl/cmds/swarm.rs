use crate::repl::LiveCli;
use crate::runtime_bridge;

impl LiveCli {
    pub(crate) fn run_loop(&self, objective: Option<&str>, budget: Option<f64>) -> Result<(), Box<dyn std::error::Error>> {
        let objective = objective.unwrap_or("Solve the problem");
        if let Some(b) = budget {
            println!("Orchestrating swarm to: {} (Budget: {:.2}M tokens)", objective, b);
        } else {
            println!("Orchestrating swarm to: {}", objective);
        }
        
        // Build an ApiClient for the orchestrator
        let (_, tool_registry) = runtime_bridge::build_runtime_plugin_state()?;
        let client = runtime_bridge::DefaultRuntimeClient::new(
            self.model.clone(),
            true, // enable_tools
            false, // emit_output
            self.allowed_tools.clone(),
            tool_registry,
            None, // progress_reporter
            runtime::RuntimeFeatureConfig::default(),
            self.tx.clone(),
        )?;
        
        let swarm_objective = swarm::SwarmObjective {
            description: objective.to_string(),
            budget,
        };
        let mut orchestrator = swarm::SwarmOrchestrator::new(
            self.runtime.session().clone(),
            swarm_objective,
            Box::new(client),
        );
        
        tokio::runtime::Runtime::new()?.block_on(async {
            orchestrator.start().await.expect("Failed to start SwarmOrchestrator");
            
            // Wait for plan approval
            if orchestrator.status() == swarm::SwarmStatus::Planning {
                println!("\n[Architect] Plan generated and written to .kla/sessions/PLAN.md");
                println!("Please review and edit the plan. You can use the Notebook UI Plan Editor.");
                println!("Type 'approve' to execute the swarm, or 'cancel' to abort: ");
                
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).expect("Failed to read input");
                if input.trim().to_lowercase() == "approve" {
                    orchestrator.approve_plan().await.expect("Failed to approve plan");
                } else {
                    println!("Swarm execution cancelled.");
                    return;
                }
            }

            while orchestrator.status() == swarm::SwarmStatus::Running {
                // Tick will try to spawn subagents for pending tasks
                orchestrator.tick().await.expect("Tick failed");
                
                // TODO: Monitor spawned subagents, wait for them, collect results,
                // and call orchestrator.complete_task or fail_task.
                // For now, we simulate success for demo if it has agents.
                let agents = orchestrator.agents().to_vec();
                if !agents.is_empty() {
                    for (i, agent) in agents.iter().enumerate() {
                        if agent.status == "running" {
                            // Pretend the agent completed successfully since we're stubbing
                            orchestrator.complete_task(i, "Success".to_string()).await.expect("Failed to complete task");
                        }
                    }
                }
                
                // Real loop would block / sleep until an agent finishes via polling manifests.
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });
        
        println!("Swarm orchestrator finished with status: {:?}", orchestrator.status());
        Ok(())
    }
}