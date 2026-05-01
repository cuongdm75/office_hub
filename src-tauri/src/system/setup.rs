use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn, error};

pub async fn ensure_ollama_installed() -> anyhow::Result<()> {
    info!("Checking if Ollama is installed...");
    
    #[cfg(windows)]
    {
        let output = Command::new("ollama").arg("--version").output();
        
        if let Ok(out) = output {
            if out.status.success() {
                info!("Ollama is already installed: {}", String::from_utf8_lossy(&out.stdout).trim());
                return Ok(());
            }
        }
        
        info!("Ollama not found. Downloading Ollama installer...");
        let temp_dir = std::env::temp_dir();
        let installer_path = temp_dir.join("OllamaSetup.exe");
        
        match download_file("https://ollama.com/download/OllamaSetup.exe", &installer_path).await {
            Ok(_) => {
                info!("Ollama installer downloaded to {}. Executing...", installer_path.display());
                
                // Execute installer silently if possible, or just normally
                let status = Command::new(&installer_path)
                    .arg("/SILENT")
                    .status();
                    
                match status {
                    Ok(s) if s.success() => {
                        info!("Ollama installed successfully.");
                    }
                    Ok(s) => {
                        warn!("Ollama installer exited with status: {}", s);
                    }
                    Err(e) => {
                        error!("Failed to execute Ollama installer: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to download Ollama installer: {}", e);
            }
        }
    }
    
    #[cfg(not(windows))]
    {
        warn!("Auto-install of Ollama is only supported on Windows for now.");
    }
    
    Ok(())
}

async fn download_file(url: &str, dest: &PathBuf) -> anyhow::Result<()> {
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to download file: HTTP {}", response.status());
    }
    
    let content = response.bytes().await?;
    std::fs::write(dest, content)?;
    Ok(())
}
