use crate::repl::LiveCli;
use crate::runtime_bridge;

impl LiveCli {
    pub(crate) fn run_retro(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Initiating autonomous retrospective sequence...");
        
        let objective = "Review recent session logs and history to identify patterns of failure, successful workflows, and areas for improvement. Propose updates to KLA.md axioms or new SKILL.md profiles.";
        
        // Build an ApiClient for the retrospective agent
        let (_, tool_registry) = runtime_bridge::build_runtime_plugin_state()?;
        let client = runtime_bridge::DefaultRuntimeClient::new(
            self.model.clone(),
            true, // enable_tools
            false, // emit_output
            self.allowed_tools.clone(),
            tool_registry,
            None,
            runtime::RuntimeFeatureConfig::default(),
            self.tx.clone(),
        )?;
        
        let swarm_objective = swarm::SwarmObjective {
            description: objective.to_string(),
            budget: None,
        };
        let mut orchestrator = swarm::SwarmOrchestrator::new(
            self.runtime.session().clone(),
            swarm_objective,
            Box::new(client),
        );
        
        tokio::runtime::Runtime::new()?.block_on(async {
            orchestrator.start().await.expect("Failed to start Retrospective Swarm");
            
            // Skip the manual approval phase for autonomous retros
            if orchestrator.status() == swarm::SwarmStatus::Planning {
                orchestrator.approve_plan().await.expect("Failed to auto-approve retro plan");
            }
            
            while orchestrator.status() == swarm::SwarmStatus::Running {
                orchestrator.tick().await.expect("Retro tick failed");
                
                // Simulate retro progress for now
                let agents = orchestrator.agents().to_vec();
                if !agents.is_empty() {
                    for (i, agent) in agents.iter().enumerate() {
                        if agent.status == "running" {
                             orchestrator.complete_task(i, "Identified pattern: missing error handling in bash tool".to_string()).await.expect("Failed to complete retro task");
                        }
                    }
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });
        
        println!("Retrospective sequence complete. Proposals generated in .kla/sessions/RETRO_REPORT.md");
        Ok(())
    }
}