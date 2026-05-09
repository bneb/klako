use swarm::{SwarmOrchestrator, SwarmObjective, SwarmStatus, SwarmTaskStatus};
use runtime::{ApiClient, ApiRequest, AssistantEvent, RuntimeError, Session};

#[derive(Clone)]
struct MockApiClient;
#[async_trait::async_trait]
impl ApiClient for MockApiClient {
    async fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let block = &request.messages.last().unwrap().blocks[0];
        let prompt = if let runtime::ContentBlock::Text { text } = block {
            text.clone()
        } else {
            String::new()
        };
        
        if prompt.contains("Decompose this objective") {
            Ok(vec![
                AssistantEvent::TextDelta(r#"[{"description": "De-risk the module"}]"#.to_string()),
                AssistantEvent::MessageStop
            ])
        } else if prompt.contains("Project Axioms") || prompt.contains("validate the changes") {
            Ok(vec![
                AssistantEvent::TextDelta(r#"{"passed": true, "reasoning": "Mock pass"}"#.to_string()),
                AssistantEvent::MessageStop
            ])
        } else {
             Ok(vec![
                AssistantEvent::TextDelta(r#"[{"description": "Task"}]"#.to_string()),
                AssistantEvent::MessageStop
            ])
        }
    }
    fn set_model(&mut self, _model: String) {}
}

#[tokio::test]
async fn test_swarm_orchestrator_enforces_budget() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test budget".to_string(),
        budget: Some(-1.0), // Force failure
    };
    
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _ = std::fs::remove_dir_all(".kla-agents");
    std::fs::write("KLA.md", "Axiom 1: Always be helpful.").unwrap();
    
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient)).await;
    orchestrator.start().await.expect("Start failed");
    orchestrator.approve_plan().await.expect("Approve failed");
    
    // The first tick should fail because the "decompose" call already used some tokens (in theory)
    // or we can simulate usage by adding a session.
    
    // For now, let's just run it and see if it fails once we implement the check.
    let res = orchestrator.tick().await;
    assert!(res.is_err(), "Orchestrator should fail when budget is exceeded");
    assert!(res.unwrap_err().contains("Budget exceeded"));
}

#[tokio::test]
async fn test_swarm_orchestrator_waits_for_agent_completion() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test waiting".to_string(),
        budget: None,
    };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _ = std::fs::remove_dir_all(".kla-agents");
    std::fs::write("KLA.md", "Axiom 1: Always be helpful.").unwrap();
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient)).await;
    orchestrator.start().await.expect("Start failed");
    orchestrator.approve_plan().await.expect("Approve failed");
    
    // 1. First tick spawns the agent
    orchestrator.tick().await.expect("Tick 1 failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Running);
    let agent_id = orchestrator.agents()[0].id.clone();
    
    // 2. Second tick: agent is still running
    orchestrator.tick().await.expect("Tick 2 failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Running, "Task should still be running");
    
    // 3. Simulate agent completion on disk
    let manifest_path = std::path::PathBuf::from(format!(".kla-agents/{}.json", agent_id));
    std::fs::create_dir_all(".kla-agents").ok();
    let manifest_json = serde_json::json!({
        "agentId": agent_id,
        "status": "completed",
        "name": "Test",
        "description": "Test",
        "outputFile": "test.out",
        "manifestFile": manifest_path.to_string_lossy(),
        "createdAt": "2026-05-08T00:00:00Z"
    });
    std::fs::write(&manifest_path, serde_json::to_string(&manifest_json).unwrap()).unwrap();
    
    // 4. Third tick: orchestrator should detect completion and move to VerifyingAxioms
    orchestrator.tick().await.expect("Tick 3 failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::VerifyingAxioms);

    // 5. Fourth tick: run axiom validation
    orchestrator.tick().await.expect("Tick 4 failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Completed);
    
    let _ = std::fs::remove_file(manifest_path);
    let _ = std::fs::remove_dir_all(".kla-agents");
    let _ = std::fs::remove_file("KLA.md");
}

#[tokio::test]
async fn test_swarm_orchestrator_empirical_verification_lifecycle() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test verification".to_string(),
        budget: None,
    };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _ = std::fs::remove_dir_all(".kla-agents");
    std::fs::write("KLA.md", "Axiom 1: Always be helpful.").unwrap();
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient)).await;
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
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::VerifyingAxioms);
    
    orchestrator.tick().await.expect("Tick 3 failed"); // Run axiom validation
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Completed);
    
    orchestrator.tick().await.expect("Tick 4 failed"); // Transition status to Completed
    assert_eq!(orchestrator.status(), SwarmStatus::Completed);
    let _ = std::fs::remove_file("KLA.md");
}

#[tokio::test]
async fn test_swarm_orchestrator_verification_failure() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "Test failure".to_string(),
        budget: None,
    };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _ = std::fs::remove_dir_all(".kla-agents");
    std::fs::write("KLA.md", "Axiom 1: Always be helpful.").unwrap();
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient)).await;
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
        panic!("Task should have failed verification, instead was {:?}", orchestrator.tasks()[0].status);
    }
    let _ = std::fs::remove_file("KLA.md");
}

#[tokio::test]
async fn test_swarm_orchestrator_full_lifecycle() {
    let session = Session::new();
    let objective = SwarmObjective {
        description: "De-risk Klako".to_string(),
        budget: None,
    };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _ = std::fs::remove_dir_all(".kla-agents");
    std::fs::write("KLA.md", "Axiom 1: Always be helpful.").unwrap();
    let mut orchestrator = SwarmOrchestrator::new(session, objective, Box::new(MockApiClient)).await;
    orchestrator.start().await.expect("Start failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Planning);
    assert_eq!(orchestrator.tasks().len(), 1);
    
    orchestrator.approve_plan().await.expect("Approve failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Running);
    
    // Tick 1: Spawn agent
    orchestrator.tick().await.expect("Tick 1 failed");
    
    // Tick 2: Complete task (manually for this test)
    orchestrator.complete_task(0, "Success".to_string()).await.expect("Complete failed");
    
    // Tick 3: Axiom Validation
    orchestrator.tick().await.expect("Tick 3 failed");
    assert_eq!(orchestrator.tasks()[0].status, SwarmTaskStatus::Completed);
    
    // Tick 4: Status update
    orchestrator.tick().await.expect("Tick 4 failed");
    assert_eq!(orchestrator.status(), SwarmStatus::Completed);
    let _ = std::fs::remove_file("KLA.md");
}
