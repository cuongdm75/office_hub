// ============================================================================
// Office Hub – agents/outlook/mod.rs
//
// Outlook Agent – MS Graph API Integration (Phase 3)
// ============================================================================

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn, error};
use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlookAgentConfig {
    pub primary_account: Option<String>,
    pub max_emails_per_fetch: usize,
    pub send_email_hitl_level: String,
    pub use_com_fallback: bool,
}

impl Default for OutlookAgentConfig {
    fn default() -> Self {
        Self {
            primary_account: None,
            max_emails_per_fetch: 20,
            send_email_hitl_level: "high".to_string(),
            use_com_fallback: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// COM Automation Helpers
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn try_com_fallback_read_inbox(max_results: u64) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for read_inbox via PowerShell...");
    
    let ps_script = format!(
        r#"
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {{
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        }} catch {{
            $outlook = New-Object -ComObject Outlook.Application
        }}
        $namespace = $outlook.GetNamespace("MAPI")
        $inbox = $namespace.GetDefaultFolder(6) # olFolderInbox
        $items = $inbox.Items
        $items.Sort("[ReceivedTime]", $true)
        $result = @()
        $count = 0
        foreach ($item in $items) {{
            if ($count -ge {}) {{ break }}
            if ($item.UnRead -eq $true) {{
                $result += @{{
                    subject = $item.Subject
                    sender = $item.SenderName
                    bodyPreview = $item.Body.Substring(0, [math]::Min($item.Body.Length, 200))
                    isRead = $item.UnRead
                }}
                $count++
            }}
        }}
        $result | ConvertTo-Json -Depth 3 -Compress
        "#,
        max_results
    );

    use base64::{Engine as _, engine::general_purpose};
    let mut utf16_bytes = Vec::new();
    for c in ps_script.encode_utf16() {
        utf16_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let encoded_cmd = general_purpose::STANDARD.encode(&utf16_bytes);

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-EncodedCommand", &encoded_cmd])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let emails: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!([]));
    let count = emails.as_array().map(|a| a.len()).unwrap_or(0);

    let mut list = String::new();
    if let Some(arr) = emails.as_array() {
        for e in arr {
            let subj = e["subject"].as_str().unwrap_or("(no subject)");
            let sender = e["sender"].as_str().unwrap_or("");
            let preview = e["bodyPreview"].as_str().unwrap_or("");
            list.push_str(&format!("- **{}** (from: {})\n  _{}_\n", subj, sender, preview.replace('\n', " ")));
        }
    }

    Ok(AgentOutput {
        content: format!("Đã lấy {} email từ Inbox qua COM Automation:\n\n{}", count, list),
        committed: true,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "read_inbox",
            "fallback": "com_automation",
            "count": count,
            "emails": emails
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_read_inbox(_max_results: u64) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_search_emails(query: &str, max_results: u64) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for search_emails via PowerShell...");
    
    let safe_query = query.replace("'", "''");

    let ps_script = format!(
        r#"
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {{
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        }} catch {{
            $outlook = New-Object -ComObject Outlook.Application
        }}
        $namespace = $outlook.GetNamespace("MAPI")
        $inbox = $namespace.GetDefaultFolder(6) # olFolderInbox
        $filter = "@SQL=""urn:schemas:httpmail:subject"" like '%{}%' OR ""urn:schemas:httpmail:textdescription"" like '%{}%'"
        $items = $inbox.Items.Restrict($filter)
        $items.Sort("[ReceivedTime]", $true)
        $result = @()
        $count = 0
        foreach ($item in $items) {{
            if ($count -ge {}) {{ break }}
            $bodyPreview = ""
            if ($item.Body) {{
                $bodyPreview = $item.Body.Substring(0, [math]::Min($item.Body.Length, 2000))
            }}
            $result += @{{
                subject = $item.Subject
                sender = $item.SenderName
                bodyPreview = $bodyPreview
                isRead = $item.UnRead
                id = $item.EntryID
            }}
            $count++
        }}
        $result | ConvertTo-Json -Depth 3 -Compress
        "#,
        safe_query, safe_query, max_results
    );

    use base64::{Engine as _, engine::general_purpose};
    let mut utf16_bytes = Vec::new();
    for c in ps_script.encode_utf16() {
        utf16_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let encoded_cmd = general_purpose::STANDARD.encode(&utf16_bytes);

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-EncodedCommand", &encoded_cmd])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let emails_val: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!([]));
    
    // ConvertTo-Json in PowerShell 5.1 unrolls single-element arrays into objects
    let emails_arr = if emails_val.is_array() {
        emails_val.as_array().unwrap().clone()
    } else if emails_val.is_object() {
        vec![emails_val.clone()]
    } else {
        vec![]
    };

    let count = emails_arr.len();

    let mut list = String::new();
    for e in &emails_arr {
        let subj = e["subject"].as_str().unwrap_or("(no subject)");
        let sender = e["sender"].as_str().unwrap_or("");
        let preview = e["bodyPreview"].as_str().unwrap_or("");
        list.push_str(&format!("- **{}** (from: {})\n  _{}_\n", subj, sender, preview.replace('\n', " ")));
    }

    Ok(AgentOutput {
        content: format!("🔍 Tìm \"{}\" → {} kết quả (qua COM):\n{}", query, count, list),
        committed: false,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "search_emails",
            "fallback": "com_automation",
            "count": count,
            "emails": emails_val
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_search_emails(_query: &str, _max_results: u64) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_read_email_by_id(id: &str) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for read_email_by_id via PowerShell...");
    
    let safe_id = id.replace("'", "''");

    let ps_script = format!(
        r#"
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {{
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        }} catch {{
            $outlook = New-Object -ComObject Outlook.Application
        }}
        $namespace = $outlook.GetNamespace("MAPI")
        $item = $namespace.GetItemFromID('{}')
        $bodyPreview = ""
        if ($item.Body) {{
            $bodyPreview = $item.Body.Substring(0, [math]::Min($item.Body.Length, 4000))
        }}
        $result = @{{
            subject = $item.Subject
            sender = $item.SenderName
            body = $bodyPreview
            isRead = $item.UnRead
            id = $item.EntryID
        }}
        $result | ConvertTo-Json -Depth 3 -Compress
        "#,
        safe_id
    );

    use base64::{Engine as _, engine::general_purpose};
    let mut utf16_bytes = Vec::new();
    for c in ps_script.encode_utf16() {
        utf16_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let encoded_cmd = general_purpose::STANDARD.encode(&utf16_bytes);

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-EncodedCommand", &encoded_cmd])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let email: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!({}));
    
    let subject = email["subject"].as_str().unwrap_or("(no subject)");
    let body = email["body"].as_str().unwrap_or("");
    
    Ok(AgentOutput {
        content: format!("📧 **{}**\n\n{}", subject, body),
        committed: false,
        tokens_used: None,
        metadata: Some(serde_json::json!({ "email_id": id, "subject": subject, "fallback": "com_automation" })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_read_email_by_id(_id: &str) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_send_email(to: &str, subject: &str, body: &str) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for send_email via PowerShell...");
    
    // Escape single quotes for PowerShell string
    let safe_to = to.replace("'", "''");
    let safe_subject = subject.replace("'", "''");
    let safe_body = body.replace("'", "''");

    let ps_script = format!(
        r#"
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {{
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        }} catch {{
            $outlook = New-Object -ComObject Outlook.Application
        }}
        $mail = $outlook.CreateItem(0) # olMailItem
        $mail.To = '{}'
        $mail.Subject = '{}'
        $mail.Body = '{}'
        $mail.Send()
        "#,
        safe_to, safe_subject, safe_body
    );

    use base64::{Engine as _, engine::general_purpose};
    let mut utf16_bytes = Vec::new();
    for c in ps_script.encode_utf16() {
        utf16_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let encoded_cmd = general_purpose::STANDARD.encode(&utf16_bytes);

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-EncodedCommand", &encoded_cmd])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    Ok(AgentOutput {
        content: format!("Đã gửi email tới '{}' với tiêu đề '{}' qua COM Automation.", to, subject),
        committed: true,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "send_email",
            "fallback": "com_automation",
            "to": to,
            "subject": subject
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_send_email(_to: &str, _subject: &str, _body: &str) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_create_event(subject: &str, body: &str, start: &str, duration_mins: u64) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for create_calendar_event via PowerShell...");
    
    let safe_subject = subject.replace("'", "''");
    let safe_body = body.replace("'", "''");
    let safe_start = start.replace("'", "''");

    let ps_script = format!(
        r#"
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {{
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        }} catch {{
            $outlook = New-Object -ComObject Outlook.Application
        }}
        $appointment = $outlook.CreateItem(1) # olAppointmentItem
        $appointment.Subject = '{}'
        $appointment.Body = '{}'
        $appointment.Start = '{}'
        $appointment.Duration = {}
        $appointment.Save()
        "#,
        safe_subject, safe_body, safe_start, duration_mins
    );

    use base64::{Engine as _, engine::general_purpose};
    let mut utf16_bytes = Vec::new();
    for c in ps_script.encode_utf16() {
        utf16_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let encoded_cmd = general_purpose::STANDARD.encode(&utf16_bytes);

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-EncodedCommand", &encoded_cmd])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    Ok(AgentOutput {
        content: format!("Đã tạo lịch '{}' qua COM Automation.", subject),
        committed: true,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "create_calendar_event",
            "fallback": "com_automation",
            "subject": subject
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_create_event(_subject: &str, _body: &str, _start: &str, _duration_mins: u64) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_read_calendar(days_ahead: u64, max_results: u64) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for read_calendar via PowerShell...");
    
    // Call the calendar.ps1 script
    let mut script_path = std::env::current_dir()?;
    script_path.push(".agent");
    script_path.push("skills");
    script_path.push("outlook-master");
    script_path.push("scripts");
    script_path.push("calendar.ps1");

    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", &script_path.to_string_lossy(),
            "-Action", "GetEvents",
            "-DaysAhead", &days_ahead.to_string(),
            "-MaxResults", &max_results.to_string()
        ])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let events_val: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!([]));
    
    let events_arr = if events_val.is_array() {
        events_val.as_array().unwrap().clone()
    } else if events_val.is_object() {
        vec![events_val.clone()]
    } else {
        vec![]
    };

    let count = events_arr.len();
    let mut list = String::new();
    for e in &events_arr {
        let subj = e["Subject"].as_str().unwrap_or("(no subject)");
        let start = e["Start"].as_str().unwrap_or("");
        let loc = e["Location"].as_str().unwrap_or("");
        list.push_str(&format!("- **{}** (Bắt đầu: {})\n  _Địa điểm: {}_\n", subj, start, loc));
    }

    Ok(AgentOutput {
        content: format!("📅 Đã lấy {} lịch họp sắp tới qua COM:\n\n{}", count, list),
        committed: false,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "read_calendar",
            "fallback": "com_automation",
            "count": count,
            "events": events_val
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_read_calendar(_days_ahead: u64, _max_results: u64) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}

#[cfg(windows)]
fn try_com_fallback_read_tasks(max_results: u64) -> anyhow::Result<AgentOutput> {
    info!("Attempting COM Automation fallback for read_tasks via PowerShell...");
    
    // Call the calendar.ps1 script
    let mut script_path = std::env::current_dir()?;
    script_path.push(".agent");
    script_path.push("skills");
    script_path.push("outlook-master");
    script_path.push("scripts");
    script_path.push("calendar.ps1");

    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", &script_path.to_string_lossy(),
            "-Action", "GetTasks",
            "-MaxResults", &max_results.to_string()
        ])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("PowerShell COM error: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tasks_val: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!([]));
    
    let tasks_arr = if tasks_val.is_array() {
        tasks_val.as_array().unwrap().clone()
    } else if tasks_val.is_object() {
        vec![tasks_val.clone()]
    } else {
        vec![]
    };

    let count = tasks_arr.len();
    let mut list = String::new();
    for t in &tasks_arr {
        let subj = t["Subject"].as_str().unwrap_or("(no subject)");
        let due = t["DueDate"].as_str().unwrap_or("Không có hạn");
        let _status = t["Status"].as_i64().unwrap_or(0);
        let pct = t["PercentComplete"].as_i64().unwrap_or(0);
        list.push_str(&format!("- **{}** (Hạn: {}) - Đã hoàn thành {}%\n", subj, due, pct));
    }

    Ok(AgentOutput {
        content: format!("📋 Đã lấy {} công việc (Tasks) qua COM:\n\n{}", count, list),
        committed: false,
        tokens_used: None,
        metadata: Some(serde_json::json!({
            "action": "read_tasks",
            "fallback": "com_automation",
            "count": count,
            "tasks": tasks_val
        })),
    })
}

#[cfg(not(windows))]
fn try_com_fallback_read_tasks(_max_results: u64) -> anyhow::Result<AgentOutput> {
    Err(anyhow::anyhow!("COM Automation is only supported on Windows"))
}


// ─────────────────────────────────────────────────────────────────────────────
// OutlookAgent
// ─────────────────────────────────────────────────────────────────────────────

pub struct OutlookAgent {
    id: AgentId,
    status: AgentStatus,
    config: OutlookAgentConfig,
    total_tasks: u64,
    error_count: u32,
    last_used: Option<DateTime<Utc>>,
    client: Client,
}

impl Default for OutlookAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl OutlookAgent {
    pub fn new() -> Self {
        Self::with_config(OutlookAgentConfig::default())
    }

    pub fn with_config(config: OutlookAgentConfig) -> Self {
        Self {
            id: AgentId::custom("outlook"),
            status: AgentStatus::Idle,
            config,
            total_tasks: 0,
            error_count: 0,
            last_used: None,
            client: Client::new(),
        }
    }

    async fn fetch_graph_token(&self) -> anyhow::Result<String> {
        // 1. Try to read from cache
        let cache_path = Self::get_token_cache_path();
        if let Ok(data) = fs::read_to_string(&cache_path) {
            if let Ok(cache) = serde_json::from_str::<serde_json::Value>(&data) {
                let expires_at = cache["expires_at"].as_i64().unwrap_or(0);
                let now = Utc::now().timestamp();
                if now < expires_at - 300 {
                    if let Some(token) = cache["access_token"].as_str() {
                        return Ok(token.to_string());
                    }
                } else if let Some(refresh) = cache["refresh_token"].as_str() {
                    // Try refresh token
                    info!("Access token expired, attempting to refresh...");
                    if let Ok(new_token) = self.refresh_graph_token(refresh).await {
                        return Ok(new_token);
                    }
                }
            }
        }

        // 2. Perform Device Code Flow
        info!("Initiating MS Graph Device Code Flow...");
        let client_id = std::env::var("MS_GRAPH_CLIENT_ID").unwrap_or_else(|_| "YOUR_CLIENT_ID_HERE".to_string());
        if client_id == "YOUR_CLIENT_ID_HERE" {
            warn!("MS_GRAPH_CLIENT_ID not set. Using mock token.");
            return Ok("mock_oauth_token_123".to_string());
        }

        let tenant = "common";
        let scope = "offline_access Mail.Read Mail.Send";

        let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", tenant);
        let params = [
            ("client_id", client_id.as_str()),
            ("scope", scope),
        ];

        let res = self.client.post(&device_code_url).form(&params).send().await?;
        let device_code_data: serde_json::Value = res.json().await?;

        if let (Some(user_code), Some(device_code), Some(verification_uri), Some(message), Some(interval)) = (
            device_code_data["user_code"].as_str(),
            device_code_data["device_code"].as_str(),
            device_code_data["verification_uri"].as_str(),
            device_code_data["message"].as_str(),
            device_code_data["interval"].as_u64(),
        ) {
            info!("ACTION REQUIRED: {}", message);
            // In a real app, we would emit this to the frontend. For now, log it.
            println!("============================================================");
            println!("👉 PLEASE LOG IN TO MICROSOFT GRAPH");
            println!("1. Go to: {}", verification_uri);
            println!("2. Enter the code: {}", user_code);
            println!("============================================================");

            let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", tenant);
            let mut attempts = 0;
            let max_attempts = 60; // e.g. 5 minutes total with 5s interval

            while attempts < max_attempts {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                attempts += 1;

                let token_params = [
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", client_id.as_str()),
                    ("device_code", device_code),
                ];

                let token_res = self.client.post(&token_url).form(&token_params).send().await?;
                let token_data: serde_json::Value = token_res.json().await?;

                if let Some(access_token) = token_data["access_token"].as_str() {
                    info!("Successfully acquired MS Graph token.");
                    let expires_in = token_data["expires_in"].as_i64().unwrap_or(3600);
                    let refresh_token = token_data["refresh_token"].as_str().unwrap_or("");
                    
                    let cache = serde_json::json!({
                        "access_token": access_token,
                        "refresh_token": refresh_token,
                        "expires_at": Utc::now().timestamp() + expires_in
                    });
                    fs::write(&cache_path, cache.to_string()).ok();

                    return Ok(access_token.to_string());
                } else if let Some(error) = token_data["error"].as_str() {
                    if error != "authorization_pending" {
                        return Err(anyhow::anyhow!("Device code flow error: {}", error));
                    }
                }
            }
            Err(anyhow::anyhow!("Device code flow timed out."))
        } else {
            Err(anyhow::anyhow!("Invalid response from devicecode endpoint"))
        }
    }

    async fn refresh_graph_token(&self, refresh_token: &str) -> anyhow::Result<String> {
        let client_id = std::env::var("MS_GRAPH_CLIENT_ID").unwrap_or_else(|_| "YOUR_CLIENT_ID_HERE".to_string());
        let token_url = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
        
        let params = [
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token),
        ];

        let res = self.client.post(token_url).form(&params).send().await?;
        let token_data: serde_json::Value = res.json().await?;

        if let Some(access_token) = token_data["access_token"].as_str() {
            let expires_in = token_data["expires_in"].as_i64().unwrap_or(3600);
            let new_refresh_token = token_data["refresh_token"].as_str().unwrap_or(refresh_token);
            
            let cache = serde_json::json!({
                "access_token": access_token,
                "refresh_token": new_refresh_token,
                "expires_at": Utc::now().timestamp() + expires_in
            });
            fs::write(Self::get_token_cache_path(), cache.to_string()).ok();
            
            Ok(access_token.to_string())
        } else {
            Err(anyhow::anyhow!("Failed to refresh token: {:?}", token_data))
        }
    }

    fn get_token_cache_path() -> PathBuf {
        let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("office-hub");
        fs::create_dir_all(&path).ok();
        path.push("msgraph_token.json");
        path
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl Agent for OutlookAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Outlook Agent"
    }

    fn description(&self) -> &str {
        "Đọc và gửi email qua Microsoft Graph API."
    }

    fn version(&self) -> &str {
        "0.3.0"
    }

    fn supported_actions(&self) -> Vec<String> {
        crate::agent_actions![
            "read_inbox",
            "send_email",
            "read_email_by_id",
            "reply_email",
            "search_emails",
            "create_calendar_event",
            "read_calendar",
            "read_tasks"
        ]
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "read_inbox".to_string(),
                description: "Đọc email trong hộp thư đến. Tham số: `unread_only` (bool), `max_results` (số lượng).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "unread_only": { "type": "boolean" },
                        "max_results": { "type": "integer" }
                    }
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "send_email".to_string(),
                description: "Gửi email mới. Tham số: `to` (địa chỉ), `subject` (tiêu đề), `body` (nội dung).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string" },
                        "subject": { "type": "string" },
                        "body": { "type": "string" }
                    },
                    "required": ["to", "subject", "body"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "read_email_by_id".to_string(),
                description: "Đọc chi tiết một email bằng ID. Tham số: `email_id`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "email_id": { "type": "string" }
                    },
                    "required": ["email_id"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "reply_email".to_string(),
                description: "Trả lời một email. Tham số: `email_id`, `body` (nội dung trả lời).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "email_id": { "type": "string" },
                        "body": { "type": "string" }
                    },
                    "required": ["email_id", "body"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "search_emails".to_string(),
                description: "Tìm kiếm email. Tham số: `query` (từ khóa), `max_results`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "max_results": { "type": "integer" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "create_calendar_event".to_string(),
                description: "Tạo lịch họp/sự kiện. Tham số: `subject`, `body`, `start` (thời gian ISO8601), `duration_mins`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "subject": { "type": "string" },
                        "body": { "type": "string" },
                        "start": { "type": "string" },
                        "duration_mins": { "type": "integer" }
                    },
                    "required": ["subject", "start"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "read_calendar".to_string(),
                description: "Đọc lịch họp sắp tới. Tham số: `days_ahead`, `max_results`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "days_ahead": { "type": "integer" },
                        "max_results": { "type": "integer" }
                    }
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "read_tasks".to_string(),
                description: "Đọc danh sách công việc (Tasks). Tham số: `max_results`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "max_results": { "type": "integer" }
                    }
                }),
                tags: vec![],
            }
        ]
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("OutlookAgent initialising (MS Graph API)…");
        self.status = AgentStatus::Idle;
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        info!("OutlookAgent shutting down");
        self.status = AgentStatus::Idle;
        Ok(())
    }

    #[instrument(skip(self, task), fields(task_id = %task.task_id))]
    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        self.total_tasks += 1;
        self.last_used = Some(Utc::now());
        self.status = AgentStatus::Busy;

        let result = match task.action.as_str() {
            "read_inbox"            => self.handle_read_inbox(&task).await,
            "send_email"            => self.handle_send_email(&task).await,
            "read_email_by_id"      => self.handle_read_email_by_id(&task).await,
            "reply_email"           => self.handle_reply_email(&task).await,
            "search_emails"         => self.handle_search_emails(&task).await,
            "create_calendar_event" => self.handle_create_calendar_event(&task).await,
            "read_calendar"         => self.handle_read_calendar(&task).await,
            "read_tasks"            => self.handle_read_tasks(&task).await,
            unknown => {
                self.error_count += 1;
                Err(anyhow::anyhow!("OutlookAgent does not support action '{}'", unknown))
            }
        };

        self.status = AgentStatus::Idle;
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Action handlers
// ─────────────────────────────────────────────────────────────────────────────

impl OutlookAgent {
    async fn handle_read_inbox(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let unread_only = task.parameters.get("unread_only").and_then(|v| v.as_bool()).unwrap_or(true);
        let max_results = task.parameters.get("max_results").and_then(|v| v.as_u64()).unwrap_or(self.config.max_emails_per_fetch as u64);

        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        // 1. Try MS Graph API
        if token == "mock_oauth_token_123" {
            warn!("MS Graph token is invalid/mocked.");
            // 2. Fallback to COM if enabled
            if self.config.use_com_fallback {
                match try_com_fallback_read_inbox(max_results) {
                    Ok(out) => return Ok(out),
                    Err(e) => {
                        error!("COM Fallback failed: {}", e);
                        return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed. Please check your token or Outlook client."));
                    }
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let filter = if unread_only { "?$filter=isRead eq false" } else { "" };
        let url = format!("https://graph.microsoft.com/v1.0/me/messages{}", filter);

        info!("Calling MS Graph API to read inbox: {}", url);
        
        let response = self.client.get(&url).bearer_auth(&token).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("MS Graph API error: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;
        let emails = data["value"].as_array().cloned().unwrap_or_default();
        
        let mut parsed_emails = Vec::new();
        let mut count = 0;
        for item in emails {
            if count >= max_results { break; }
            parsed_emails.push(serde_json::json!({
                "subject": item["subject"].as_str().unwrap_or(""),
                "sender": item["sender"]["emailAddress"]["address"].as_str().unwrap_or(""),
                "bodyPreview": item["bodyPreview"].as_str().unwrap_or(""),
                "isRead": item["isRead"].as_bool().unwrap_or(false),
            }));
            count += 1;
        }

        let mut list = String::new();
        for e in &parsed_emails {
            let subj = e["subject"].as_str().unwrap_or("(no subject)");
            let sender = e["sender"].as_str().unwrap_or("");
            let preview = e["bodyPreview"].as_str().unwrap_or("");
            list.push_str(&format!("- **{}** (from: {})\n  _{}_\n", subj, sender, preview.replace('\n', " ")));
        }

        Ok(AgentOutput {
            content: format!("Đã lấy {} email từ Inbox (MS Graph API):\n\n{}", parsed_emails.len(), list),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "read_inbox",
                "count": parsed_emails.len(),
                "emails": parsed_emails
            })),
        })
    }

    async fn handle_send_email(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let to = task.parameters.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let subject = task.parameters.get("subject").and_then(|v| v.as_str()).unwrap_or("");
        let body = task.parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");

        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        // 1. Try MS Graph API
        if token == "mock_oauth_token_123" {
            warn!("MS Graph token is invalid/mocked.");
            // 2. Fallback to COM if enabled
            if self.config.use_com_fallback {
                match try_com_fallback_send_email(to, subject, body) {
                    Ok(out) => return Ok(out),
                    Err(e) => {
                        error!("COM Fallback failed: {}", e);
                        return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed. Please check your token or Outlook client."));
                    }
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let url = "https://graph.microsoft.com/v1.0/me/sendMail";

        info!("Calling MS Graph API to send email to {}", to);
        let payload = serde_json::json!({
            "message": {
                "subject": subject,
                "body": {
                    "contentType": "Text",
                    "content": body
                },
                "toRecipients": [
                    {
                        "emailAddress": {
                            "address": to
                        }
                    }
                ]
            },
            "saveToSentItems": "true"
        });

        let response = self.client.post(url)
            .bearer_auth(&token)
            .json(&payload)
            .send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("MS Graph API error sending email: {}", response.status()));
        }

        Ok(AgentOutput {
            content: format!("Đã gửi email tới '{}' với tiêu đề '{}' qua MS Graph API.", to, subject),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "send_email",
                "to": to,
                "subject": subject
            })),
        })
    }

    async fn handle_read_email_by_id(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let id = task.parameters.get("email_id").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Thiếu email_id"))?;
            
        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        if token == "mock_oauth_token_123" {
            if self.config.use_com_fallback {
                match try_com_fallback_read_email_by_id(id) {
                    Ok(out) => return Ok(out),
                    Err(e) => return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed for read_email_by_id. Error: {}", e)),
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let url = format!("https://graph.microsoft.com/v1.0/me/messages/{}", id);
        let response = self.client.get(&url).bearer_auth(&token).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("MS Graph API error: {}", response.status()));
        }
        let data: serde_json::Value = response.json().await?;
        let subject = data["subject"].as_str().unwrap_or("(no subject)");
        let body = data["body"]["content"].as_str().unwrap_or("");
        Ok(AgentOutput {
            content: format!("📧 **{}**\n\n{}", subject, &body[..body.len().min(2000)]),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "email_id": id, "subject": subject })),
        })
    }

    async fn handle_reply_email(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let id = task.parameters.get("email_id").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Thiếu email_id"))?;
        let comment = task.parameters.get("body").and_then(|v| v.as_str())
            .unwrap_or(&task.message);
        let token = self.fetch_graph_token().await?;
        let url = format!("https://graph.microsoft.com/v1.0/me/messages/{}/reply", id);
        let payload = serde_json::json!({ "comment": comment });
        let res = self.client.post(&url).bearer_auth(&token).json(&payload).send().await?;
        if !res.status().is_success() {
            return Err(anyhow::anyhow!("Graph reply error: {}", res.status()));
        }
        Ok(AgentOutput {
            content: format!("✅ Đã reply email `{}`.", id),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "email_id": id })),
        })
    }

    async fn handle_search_emails(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let query = task.parameters.get("query").and_then(|v| v.as_str())
            .unwrap_or(&task.message);
        let max = task.parameters.get("max_results").and_then(|v| v.as_u64()).unwrap_or(10);
        
        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        if token == "mock_oauth_token_123" {
            if self.config.use_com_fallback {
                match try_com_fallback_search_emails(query, max) {
                    Ok(out) => return Ok(out),
                    Err(e) => return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed for search_emails. Error: {}", e)),
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/messages?$search=\"{}\"&$top={}",
            urlencoding::encode(query), max
        );
        let response = self.client.get(&url)
            .bearer_auth(&token)
            .header("ConsistencyLevel", "eventual")
            .send().await?;
            
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("MS Graph API error: {}", response.status()));
        }
        
        let data: serde_json::Value = response.json().await?;
        let emails = data["value"].as_array().cloned().unwrap_or_default();
        let list = emails.iter().map(|e| format!(
            "- **{}** (from: {})",
            e["subject"].as_str().unwrap_or(""),
            e["sender"]["emailAddress"]["address"].as_str().unwrap_or("")
        )).collect::<Vec<_>>().join("\n");
        Ok(AgentOutput {
            content: format!("🔍 Tìm \"{}\" → {} kết quả:\n{}", query, emails.len(), list),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "query": query, "count": emails.len(), "emails": emails })),
        })
    }
    async fn handle_create_calendar_event(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let subject = task.parameters.get("subject").and_then(|v| v.as_str()).unwrap_or("Họp");
        let body = task.parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let start = task.parameters.get("start").and_then(|v| v.as_str()).unwrap_or("");
        let duration_mins = task.parameters.get("duration_mins").and_then(|v| v.as_u64()).unwrap_or(60);

        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        if token == "mock_oauth_token_123" {
            if self.config.use_com_fallback {
                match try_com_fallback_create_event(subject, body, start, duration_mins) {
                    Ok(out) => return Ok(out),
                    Err(e) => return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed. Error: {}", e)),
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let url = "https://graph.microsoft.com/v1.0/me/events";

        info!("Calling MS Graph API to create calendar event");
        
        // Parse the provided start datetime string. Fallback to current time if unparseable.
        let parsed_dt = chrono::DateTime::parse_from_rfc3339(start)
            .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{}:00+07:00", start))) // rough fallback
            .unwrap_or_else(|_| chrono::Utc::now().into());

        let end_dt = parsed_dt + chrono::Duration::minutes(duration_mins as i64);

        let payload = serde_json::json!({
            "subject": subject,
            "body": {
                "contentType": "Text",
                "content": body
            },
            "start": {
                "dateTime": parsed_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "timeZone": "SE Asia Standard Time"
            },
            "end": {
                "dateTime": end_dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "timeZone": "SE Asia Standard Time"
            }
        });

        let response = self.client.post(url)
            .bearer_auth(&token)
            .json(&payload)
            .send().await?;

        if !response.status().is_success() {
            let e = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("MS Graph API error creating event: {}", e));
        }

        Ok(AgentOutput {
            content: format!("✅ Đã tạo sự kiện '{}' vào lúc {} qua MS Graph API.", subject, start),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "create_calendar_event",
                "subject": subject
            })),
        })
    }

    async fn handle_read_calendar(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let days_ahead = task.parameters.get("days_ahead").and_then(|v| v.as_u64()).unwrap_or(7);
        let max_results = task.parameters.get("max_results").and_then(|v| v.as_u64()).unwrap_or(20);

        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        if token == "mock_oauth_token_123" {
            if self.config.use_com_fallback {
                match try_com_fallback_read_calendar(days_ahead, max_results) {
                    Ok(out) => return Ok(out),
                    Err(e) => return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed. Error: {}", e)),
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        let start_datetime = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let end_datetime = (chrono::Utc::now() + chrono::Duration::days(days_ahead as i64)).format("%Y-%m-%dT%H:%M:%S").to_string();
        
        let url = format!(
            "https://graph.microsoft.com/v1.0/me/calendarview?startdatetime={}&enddatetime={}&$top={}",
            start_datetime, end_datetime, max_results
        );

        info!("Calling MS Graph API to read calendar");
        let response = self.client.get(&url).bearer_auth(&token).send().await?;

        if !response.status().is_success() {
            let e = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("MS Graph API error reading calendar: {}", e));
        }

        let data: serde_json::Value = response.json().await?;
        let events = data["value"].as_array().cloned().unwrap_or_default();
        
        let list = events.iter().map(|e| format!(
            "- **{}** (Bắt đầu: {})",
            e["subject"].as_str().unwrap_or(""),
            e["start"]["dateTime"].as_str().unwrap_or("")
        )).collect::<Vec<_>>().join("\n");

        Ok(AgentOutput {
            content: format!("📅 Đã lấy {} lịch họp sắp tới qua MS Graph API:\n\n{}", events.len(), list),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "read_calendar",
                "count": events.len(),
                "events": events
            })),
        })
    }

    async fn handle_read_tasks(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let max_results = task.parameters.get("max_results").and_then(|v| v.as_u64()).unwrap_or(20);

        let token = self.fetch_graph_token().await.unwrap_or_else(|_| "mock_oauth_token_123".to_string());
        
        if token == "mock_oauth_token_123" {
            if self.config.use_com_fallback {
                match try_com_fallback_read_tasks(max_results) {
                    Ok(out) => return Ok(out),
                    Err(e) => return Err(anyhow::anyhow!("Both MS Graph API and COM Automation failed. Error: {}", e)),
                }
            } else {
                return Err(anyhow::anyhow!("MS Graph API failed and COM fallback is disabled."));
            }
        }
        
        // Graph API for Microsoft To Do
        let url = format!("https://graph.microsoft.com/v1.0/me/todo/lists/tasks/tasks?$filter=status ne 'completed'&$top={}", max_results);

        info!("Calling MS Graph API to read tasks");
        let response = self.client.get(&url).bearer_auth(&token).send().await?;

        if !response.status().is_success() {
            let e = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("MS Graph API error reading tasks: {}", e));
        }

        let data: serde_json::Value = response.json().await?;
        let tasks = data["value"].as_array().cloned().unwrap_or_default();
        
        let list = tasks.iter().map(|t| format!(
            "- **{}** (Hạn: {})",
            t["title"].as_str().unwrap_or(""),
            t["dueDateTime"]["dateTime"].as_str().unwrap_or("Không có hạn")
        )).collect::<Vec<_>>().join("\n");

        Ok(AgentOutput {
            content: format!("📋 Đã lấy {} công việc (Tasks) qua MS Graph API:\n\n{}", tasks.len(), list),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "read_tasks",
                "count": tasks.len(),
                "tasks": tasks
            })),
        })
    }
}
