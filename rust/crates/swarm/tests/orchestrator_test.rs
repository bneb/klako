use swarm::{SwarmOrchestrator, SwarmObjective, SwarmStatus, SwarmTaskStatus};
use runtime::{ApiClient, ApiRequest, AssistantEvent, RuntimeError, Session};

struct MockApiClient;
impl ApiClient for MockApiClient {
    fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        Ok(vec![
            AssistantEvent::TextDelta(r#"[{"description": "De-risk the module"}]"#.to_string()),
            AssistantEvent::MessageStop
        ])
    }
}

#[tokio::test]
async fn test_swarm_orchestrator_full_lifecycle() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test objective".to_string(),
    };
    
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient));
    
    assert_eq!(orchestrator.status(), SwarmStatus::Idle);
    
    orchestrator.start().await.expect("Start failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Running);
    assert!(!orchestrator.tasks().is_empty());
    
    orchestrator.tick().await.expect("Tick failed");
    assert!(orchestrator.tasks().iter().any(|t| t.status == SwarmTaskStatus::Running));
    assert!(!orchestrator.agents().is_empty());
    
    orchestrator.complete_task(0, "Success".to_string()).await.expect("Complete failed");
    orchestrator.tick().await.expect("Tick failed");
    
    assert_eq!(orchestrator.status(), SwarmStatus::Completed);
}

#[tokio::test]
async fn test_swarm_orchestrator_spawns_subagent_via_tools() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Verify spawn logic".to_string(),
    };
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient));
    orchestrator.start().await.expect("Start failed");
    orchestrator.tick().await.expect("Tick failed");
    
    let agents = orchestrator.agents();
    assert_eq!(agents.len(), 1);
    assert!(agents[0].id.starts_with("agent-") || !agents[0].id.is_empty(), "Should have a real agent id");
    // Since tick should actually spawn an agent using tools::Delegate, we should verify the agent's properties.
    assert_eq!(agents[0].subagent_type, "Engineer");
}
