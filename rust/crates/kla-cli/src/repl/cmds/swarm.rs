use crate::repl::LiveCli;
use crate::bridge;

impl LiveCli {
    pub async fn run_loop(&self, objective: Option<&str>, budget: Option<f64>) -> Result<(), Box<dyn std::error::Error>> {
        let is_resume = objective.is_none();
        let objective_text = objective.unwrap_or("Continue current objective");
        
        println!("Initiating autonomous swarm loop for: {objective_text}");
        
        // Build an ApiClient for the swarm orchestrator (The Architect)
        let (feature_config, tool_registry) = bridge::build_runtime_plugin_state()?;
        let client = bridge::DefaultRuntimeClient::new(
            self.model.clone(),
            true, // enable_tools
            false, // emit_output
            self.allowed_tools.clone(),
            tool_registry,
            None,
            feature_config,
            self.tx.clone(),
            self.system_prompt.clone(),
        ).await?;
        
        let swarm_objective = swarm::SwarmObjective {
            description: objective_text.to_string(),
            budget,
        };
        let mut orchestrator = swarm::SwarmOrchestrator::new(
            self.runtime.session().clone(),
            swarm_objective,
            Box::new(client),
        ).await;
        
        if !is_resume {
            orchestrator.start().await.expect("Failed to start Swarm");
            
            println!("\nProposed Plan generated in .kla/sessions/PLAN.md");
            println!("Review and edit the plan if necessary, then type 'approve' or press Enter to continue.");
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            
            orchestrator.approve_plan().await.expect("Failed to approve plan");
        }
        
        while orchestrator.status() == swarm::SwarmStatus::Running {
            // Tick will try to spawn subagents for pending tasks AND poll running ones.
            orchestrator.tick().await.expect("Tick failed");
            
            // Real loop blocks / sleeps until an agent finishes via polling manifests (handled inside tick).
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
        
        println!("Swarm orchestrator finished.");
        Ok(())
    }
}
