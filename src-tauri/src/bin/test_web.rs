use office_hub_lib::agents::web_researcher::WebResearcherAgent;
use office_hub_lib::agents::Agent;
use office_hub_lib::orchestrator::{AgentTask, intent::Intent};

#[tokio::main]
async fn main() {
    // Setup tracing
    tracing_subscriber::fmt::init();

    let mut agent = WebResearcherAgent::with_defaults();
    
    // Execute a test task
    let task = AgentTask {
        task_id: "test-001".to_string(),
        action: "search_google".to_string(),
        intent: Intent::GeneralChat(Default::default()),
        message: "".to_string(),
        context_file: None,
        session_id: "session-test".to_string(),
        parameters: {
            let mut p = std::collections::HashMap::new();
            p.insert("query".to_string(), serde_json::json!("Rust Tauri 2.0 release"));
            p
        },
        llm_gateway: None,
        global_policy: None,
        knowledge_context: None,
        dependencies: vec![],
        parent_task_id: None,
    };
    
    println!("Executing WebResearcherAgent...");
    match agent.execute(task).await {
        Ok(output) => {
            println!("Success! Output:");
            println!("{}", output.content);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
