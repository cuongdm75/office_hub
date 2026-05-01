import io
with open('src-tauri/src/orchestrator/mod.rs', 'rb') as f:
    text = f.read().decode('utf-8')

missing_code = '''
    #[instrument(skip(self, message), fields(session = session_id))]
    pub async fn process_message(
        &mut self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
    ) -> Result<OrchestratorResponse> {
        let started_at = Utc::now();
        self.metrics.total_requests += 1;

        // ── 1. Retrieve Session ───────────────────────────────────────────────
        let session_clone = {
            let session = self
                .session_store
                .get_or_create(session_id)
                .context("Failed to retrieve/create session")?;
            debug!(turns = session.messages.len(), "Session retrieved");
            session.clone()
        };

        // ── 2. Build Agent Tool Prompt ──────────────────────────────────────────
'''

# Find the marker
marker = '        let statuses = self.agent_registry.all_statuses();'
if marker in text:
    text = text.replace(marker, missing_code + marker, 1)
    with open('src-tauri/src/orchestrator/mod.rs', 'w', encoding='utf-8') as f:
        f.write(text)
    print("Fixed!")
else:
    print("Marker not found")
