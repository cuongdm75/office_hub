use office_hub_lib::llm_gateway::LlmGateway;
use office_hub_lib::orchestrator::{HitlManager, Orchestrator, OrchestratorHandle};
use office_hub_lib::AppConfig;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // 1. Load config (reads config.yaml or uses defaults)
    let config = AppConfig::load();

    // 2. Init LLM Gateway
    let llm_gateway = Arc::new(RwLock::new(LlmGateway::new(config.llm.clone())));

    // 3. Init Orchestrator
    let hitl = Arc::new(HitlManager::new());
    let mut orchestrator = Orchestrator::new(Arc::clone(&llm_gateway), Arc::clone(&hitl));

    // Register agents
    orchestrator.agent_registry.register(Box::new(
        office_hub_lib::agents::office_master::OfficeMasterAgent::new(),
    ));
    orchestrator.agent_registry.register(Box::new(
        office_hub_lib::agents::analyst::AnalystAgent::new(),
    ));

    let handle = OrchestratorHandle::new(orchestrator);

    println!("Testing Orchestrator with message: 'Xin chao, 1 + 1 bang may?'");

    match handle
        .process_message_native(
            "test_session_001",
            "Xin chao, 1 + 1 bang may?",
            None,
            None,
            None,
        )
        .await
    {
        Ok(resp) => {
            println!("Success! Response: {}", resp.content);
            println!("   Agent used: {:?}", resp.agent_used);
            println!("   Intent:     {:?}", resp.intent);
            println!("   Tokens:     {:?}", resp.tokens_used);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
