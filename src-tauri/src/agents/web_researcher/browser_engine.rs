// ============================================================================
// Office Hub – agents/web_researcher/browser_engine.rs
//
// Obscura-based browser engine driver.
// Spawns obscura.exe as a subprocess and provides high-level fetch/scrape API.
// Zero extra Rust crates needed — uses tokio::process::Command only.
// ============================================================================

use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error, info};

/// Default timeout for a single fetch operation
const FETCH_TIMEOUT_SECS: u64 = 30;

/// Maximum text size returned to LLM (chars)
const MAX_TEXT_CHARS: usize = 50_000;

// ─────────────────────────────────────────────────────────────────────────────
// BrowserEngine
// ─────────────────────────────────────────────────────────────────────────────

/// Manages the Obscura headless browser binary.
/// Stateless — each call spawns a short-lived subprocess.
#[derive(Debug, Clone)]
pub struct BrowserEngine {
    /// Absolute path to obscura.exe
    binary_path: PathBuf,
    /// Enable stealth mode (anti-fingerprinting + tracker blocking)
    stealth: bool,
    /// Fetch timeout
    timeout_secs: u64,
}

impl BrowserEngine {
    /// Create a new engine, locating `obscura.exe` automatically.
    pub fn new() -> anyhow::Result<Self> {
        let binary_path = Self::locate_binary()?;
        info!("BrowserEngine initialised: {}", binary_path.display());
        Ok(Self {
            binary_path,
            stealth: true,
            timeout_secs: FETCH_TIMEOUT_SECS,
        })
    }

    /// Try to locate `obscura.exe` in several well-known locations.
    fn locate_binary() -> anyhow::Result<PathBuf> {
        // 1. Bundled alongside the app executable
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from));

        let candidates: Vec<PathBuf> = [
            // Bundled with Tauri app
            exe_dir.as_ref().map(|d| d.join("obscura.exe")),
            exe_dir.as_ref().map(|d| d.join("bin").join("obscura.exe")),
            // Dev workspace tools directory
            Some(PathBuf::from(r"e:\Office hub\tools\obscura\obscura.exe")),
            // System PATH
            Some(PathBuf::from("obscura.exe")),
        ]
        .into_iter()
        .flatten()
        .collect();

        for path in &candidates {
            if path.exists() {
                return Ok(path.clone());
            }
            // For PATH-based entry, try resolving
            if path.as_os_str() == "obscura.exe" {
                if which_obscura().is_some() {
                    return Ok(path.clone());
                }
            }
        }

        anyhow::bail!(
            "obscura.exe not found. Searched: {:?}\n\
             Download from https://github.com/h4ckf0r0day/obscura/releases and place in the app directory.",
            candidates
        )
    }

    /// Check that the binary is reachable (quick sanity test).
    pub async fn health_check(&self) -> bool {
        let result = Command::new(&self.binary_path)
            .arg("--help")
            .output()
            .await;
        result.is_ok()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Core fetch operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Fetch a URL and return the rendered plain-text content.
    pub async fn fetch_text(&self, url: &str) -> anyhow::Result<FetchResult> {
        info!("BrowserEngine::fetch_text → {}", url);
        let mut args = vec!["fetch", url, "--dump", "text"];
        if self.stealth {
            args.push("--stealth");
        }
        self.run_fetch(url, &args).await
    }

    /// Fetch a URL and return the rendered HTML.
    pub async fn fetch_html(&self, url: &str) -> anyhow::Result<FetchResult> {
        info!("BrowserEngine::fetch_html → {}", url);
        let mut args = vec!["fetch", url, "--dump", "html"];
        if self.stealth {
            args.push("--stealth");
        }
        self.run_fetch(url, &args).await
    }

    /// Fetch a URL and return all hyperlinks as `(url, text)` pairs.
    pub async fn fetch_links(&self, url: &str) -> anyhow::Result<Vec<(String, String)>> {
        info!("BrowserEngine::fetch_links → {}", url);
        let mut args = vec!["fetch", url, "--dump", "links"];
        if self.stealth {
            args.push("--stealth");
        }
        let result = self.run_fetch(url, &args).await?;

        // Parse: each line is "url\tanchor_text"
        let links = result
            .content
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, '\t');
                let href = parts.next()?.trim().to_string();
                let text = parts.next().unwrap_or("").trim().to_string();
                if href.starts_with("http") {
                    Some((href, text))
                } else {
                    None
                }
            })
            .collect();
        Ok(links)
    }

    /// Evaluate arbitrary JavaScript on a page and return the result.
    pub async fn eval_js(&self, url: &str, js: &str) -> anyhow::Result<FetchResult> {
        info!("BrowserEngine::eval_js → {} | js: {:.40}", url, js);
        let mut args = vec!["fetch", url, "--eval", js];
        if self.stealth {
            args.push("--stealth");
        }
        self.run_fetch(url, &args).await
    }

    /// Scrape multiple URLs in parallel (up to `concurrency` workers).
    pub async fn scrape_parallel(
        &self,
        urls: &[String],
        concurrency: usize,
        eval: Option<&str>,
    ) -> anyhow::Result<Vec<ScrapeResult>> {
        if urls.is_empty() {
            return Ok(vec![]);
        }

        let concurrency = concurrency.clamp(1, 25);

        info!(
            "BrowserEngine::scrape_parallel → {} URLs (concurrency={})",
            urls.len(),
            concurrency
        );

        // obscura scrape <url1> <url2> ... --concurrency N --format json [--eval JS] [--timeout S]
        let mut args: Vec<String> = vec!["scrape".to_string()];
        for url in urls {
            args.push(url.clone());
        }
        args.push("--concurrency".to_string());
        args.push(concurrency.to_string());
        args.push("--format".to_string());
        args.push("json".to_string());
        args.push("--timeout".to_string());
        args.push(self.timeout_secs.to_string());
        if let Some(js) = eval {
            args.push("--eval".to_string());
            args.push(js.to_string());
        }
        // Note: obscura scrape does NOT support --stealth flag

        let total_timeout = Duration::from_secs(
            self.timeout_secs.saturating_add(urls.len() as u64 * 5)
        );

        let op = Command::new(&self.binary_path)
            .args(&args)
            .output();

        let output = timeout(total_timeout, op)
            .await
            .map_err(|_| anyhow::anyhow!("scrape_parallel timed out after {}s for {} URLs", total_timeout.as_secs(), urls.len()))?
            .map_err(|e| anyhow::anyhow!("obscura spawn error: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // Parse JSON array output from obscura scrape --format json
        let results: Vec<ScrapeResult> = serde_json::from_str(&stdout).unwrap_or_else(|_| {
            // Fallback: treat as single text result
            vec![ScrapeResult {
                url: urls.first().cloned().unwrap_or_default(),
                content: stdout,
                error: None,
            }]
        });

        Ok(results)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ─────────────────────────────────────────────────────────────────────────

    async fn run_fetch(&self, url: &str, args: &[&str]) -> anyhow::Result<FetchResult> {
        let op = Command::new(&self.binary_path)
            .args(args)
            .output();

        let output = timeout(Duration::from_secs(self.timeout_secs), op)
            .await
            .map_err(|_| anyhow::anyhow!("obscura fetch timed out after {}s for {}", self.timeout_secs, url))?
            .map_err(|e| anyhow::anyhow!("obscura spawn error: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("obscura exited with {} — stderr: {}", output.status, stderr);
            anyhow::bail!("obscura fetch failed ({}): {}", output.status, stderr.trim());
        }

        let mut content = String::from_utf8_lossy(&output.stdout).to_string();

        // Extract page title from stderr log line: `Page loaded: <url> - "<title>"`
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let title = stderr
            .lines()
            .find(|l| l.contains("Page loaded:"))
            .and_then(|l| {
                let start = l.find('"')? + 1;
                let end = l.rfind('"')?;
                Some(l[start..end].to_string())
            });

        debug!("Fetched {} ({} chars)", url, content.len());

        // Truncate to avoid LLM context overflow
        if content.len() > MAX_TEXT_CHARS {
            content.truncate(MAX_TEXT_CHARS);
            content.push_str("\n... [truncated]");
        }

        Ok(FetchResult {
            url: url.to_string(),
            title,
            content,
        })
    }
}

impl Default for BrowserEngine {
    fn default() -> Self {
        Self::new().expect("BrowserEngine::default() failed to locate obscura binary")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Result types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub url: String,
    pub title: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ScrapeResult {
    pub url: String,
    pub content: String,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

fn which_obscura() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join("obscura.exe");
            if full.exists() { Some(full) } else { None }
        })
    })
}
