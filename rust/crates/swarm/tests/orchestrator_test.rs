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
        budget: None,
    };
    
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient));
    
    assert_eq!(orchestrator.status(), SwarmStatus::Idle);
    
    orchestrator.start().await.expect("Start failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Planning);
    assert!(!orchestrator.tasks().is_empty());
    
    orchestrator.approve_plan().await.expect("Approve failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Running);
    
    orchestrator.tick().await.expect("Tick failed");
    assert!(orchestrator.tasks().iter().any(|t| t.status == SwarmTaskStatus::Running));
    assert!(!orchestrator.agents().is_empty());
    
    orchestrator.complete_task(0, "Success".to_string()).await.expect("Complete failed");
    orchestrator.tick().await.expect("Tick failed");
    
    assert_eq!(orchestrator.status(), SwarmStatus::Completed);
}

#[tokio::test]
async fn test_swarm_orchestrator_empirical_verification_lifecycle() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test verification".to_string(),
        budget: None,
    };
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient));
    orchestrator.start().await.expect("Start failed");
    orchestrator.approve_plan().await.expect("Approve failed");
    
    // Manually set a verification tool for the first task
    if let Some(task) = orchestrator.tasks_mut().get_mut(0) {
        task.verification_tool = Some("Sleep".to_string());
        task.verification_input = Some(serde_json::json!({ "duration_ms": 0 }));
    }
    
    orchestrator.tick().await.expect("Tick 1 failed"); // Spawn agent
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Running);
    
    orchestrator.complete_task(0, "Done".to_string()).await.expect("Complete failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Verifying);
    
    orchestrator.tick().await.expect("Tick 2 failed"); // Run verification
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Completed);
    
    orchestrator.tick().await.expect("Tick 3 failed"); // Transition status to Completed
    assert_eq!(orchestrator.status(), SwarmStatus::Completed);
}

#[tokio::test]
async fn test_swarm_orchestrator_verification_failure() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test failure".to_string(),
        budget: None,
    };
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient));
    orchestrator.start().await.expect("Start failed");
    orchestrator.approve_plan().await.expect("Approve failed");
    
    // Manually set a verification tool that will fail (invalid tool name)
    if let Some(task) = orchestrator.tasks_mut().get_mut(0) {
        task.verification_tool = Some("NonExistentTool".to_string());
        task.verification_input = Some(serde_json::json!({}));
    }
    
    orchestrator.tick().await.expect("Tick 1 failed");
    orchestrator.complete_task(0, "Done".to_string()).await.expect("Complete failed");
    
    orchestrator.tick().await.expect("Tick 2 failed"); // Run verification (fails because tool doesn't exist)
    
    if let SwarmTaskStatus::Failed(err) = &orchestrator.tasks()[0].status {
        assert!(err.contains("Verification failed"));
        assert!(err.contains("unsupported tool"));
    } else {
        panic!("Task should have failed verification");
    }
}
