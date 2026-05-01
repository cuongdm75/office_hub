// ============================================================================
// Office Hub – system/mod.rs
//
// System Integration Layer
//
// Trách nhiệm:
//   1. System Tray icon + context menu (thu nhỏ xuống notification area)
//   2. Windows Startup registration (HKCU Run key)
//   3. Sleep / Away-mode override khi task đang chạy
//      (SetThreadExecutionState WIN32 API)
//   4. Lock-screen awareness – giữ agents hoạt động khi màn hình khoá
//   5. QR Code generation cho Mobile pairing
//   6. Tailscale integration – phát hiện IP Tailscale
//   7. Power event monitoring (sleep/wake/lock/unlock)
// ============================================================================

// Note: Module definitions are inline below (not external files)

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

pub mod setup;

// ─────────────────────────────────────────────────────────────────────────────
// SystemManager – top-level coordinator
// ─────────────────────────────────────────────────────────────────────────────

/// Coordinates all system-level integrations.
///
/// Obtained via `SystemManager::init()` at app startup.
/// Cheaply cloneable (Arc-backed).
#[derive(Clone)]
pub struct SystemManager {
    /// Is the app currently minimised to tray?
    pub tray_mode: Arc<AtomicBool>,
    /// Is sleep currently being suppressed?
    pub sleep_suppressed: Arc<AtomicBool>,
    /// Tailscale state (refreshed periodically)
    pub tailscale: Arc<tokio::sync::RwLock<tailscale::TailscaleState>>,
    /// Current network info for mobile pairing
    pub network_info: Arc<tokio::sync::RwLock<NetworkInfo>>,
    /// System config
    pub config: Arc<tokio::sync::RwLock<SystemConfig>>,
}

impl SystemManager {
    /// Initialise the system manager and probe current state.
    pub async fn init(config: SystemConfig) -> anyhow::Result<Self> {
        info!("SystemManager initialising");

        let tailscale_state = tailscale::probe().await;
        let network_info = NetworkInfo::probe(&tailscale_state, config.websocket_port);

        let mgr = Self {
            tray_mode: Arc::new(AtomicBool::new(false)),
            sleep_suppressed: Arc::new(AtomicBool::new(false)),
            tailscale: Arc::new(tokio::sync::RwLock::new(tailscale_state)),
            network_info: Arc::new(tokio::sync::RwLock::new(network_info)),
            config: Arc::new(tokio::sync::RwLock::new(config)),
        };

        info!("SystemManager ready");
        Ok(mgr)
    }

    // ── Tray ─────────────────────────────────────────────────────────────────

    pub fn set_tray_mode(&self, enabled: bool) {
        self.tray_mode.store(enabled, Ordering::Relaxed);
        debug!("Tray mode: {}", enabled);
    }

    pub fn is_tray_mode(&self) -> bool {
        self.tray_mode.load(Ordering::Relaxed)
    }

    // ── Sleep suppression ─────────────────────────────────────────────────────

    /// Prevent Windows from sleeping / turning off display while tasks are running.
    /// Returns `true` if successfully suppressed.
    pub fn suppress_sleep(&self) -> bool {
        if self.sleep_suppressed.load(Ordering::Relaxed) {
            return true; // already suppressed
        }
        let ok = power::suppress_sleep();
        if ok {
            self.sleep_suppressed.store(true, Ordering::Relaxed);
            info!("Sleep suppressed: Windows will stay awake during active tasks");
        } else {
            warn!("Failed to suppress sleep (SetThreadExecutionState returned 0)");
        }
        ok
    }

    /// Re-allow Windows to sleep normally.
    pub fn release_sleep(&self) {
        if !self.sleep_suppressed.load(Ordering::Relaxed) {
            return;
        }
        power::release_sleep();
        self.sleep_suppressed.store(false, Ordering::Relaxed);
        info!("Sleep suppression released");
    }

    // ── Startup ───────────────────────────────────────────────────────────────

    /// Register Office Hub to start with Windows (HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run).
    pub fn enable_startup() -> anyhow::Result<()> {
        startup::register()
    }

    /// Remove from Windows startup.
    pub fn disable_startup() -> anyhow::Result<()> {
        startup::unregister()
    }

    /// Check if currently registered for Windows startup.
    pub fn is_startup_enabled() -> bool {
        startup::is_registered()
    }

    // ── Network refresh ───────────────────────────────────────────────────────

    /// Refresh Tailscale state and network info (call after Tailscale changes).
    pub async fn refresh_network(&self) {
        let ts = tailscale::probe().await;
        let cfg = self.config.read().await;
        let net = NetworkInfo::probe(&ts, cfg.websocket_port);
        *self.tailscale.write().await = ts;
        *self.network_info.write().await = net;
        info!("Network info refreshed");
    }

    // ── QR Code ───────────────────────────────────────────────────────────────

    /// Generate QR code payload for mobile pairing.
    /// Returns the SVG string of the QR code.
    pub async fn generate_pairing_qr(&self) -> anyhow::Result<PairingQrPayload> {
        let net = self.network_info.read().await;
        let cfg = self.config.read().await;
        qrcode::generate_pairing_qr(&net, cfg.ws_auth_token.as_deref())
    }

    // ── Status ────────────────────────────────────────────────────────────────

    pub async fn status_json(&self) -> serde_json::Value {
        let ts = self.tailscale.read().await;
        let net = self.network_info.read().await;
        serde_json::json!({
            "trayMode":        self.is_tray_mode(),
            "sleepSuppressed": self.sleep_suppressed.load(Ordering::Relaxed),
            "startupEnabled":  Self::is_startup_enabled(),
            "tailscale":       ts.as_json(),
            "network":         net.as_json(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemConfig
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemConfig {
    /// Start with Windows
    pub startup_with_windows: bool,
    /// Minimise to tray instead of closing
    pub minimise_to_tray: bool,
    /// Suppress sleep during active tasks
    pub suppress_sleep_during_tasks: bool,
    /// Keep agents active on lock-screen
    pub agents_active_on_lockscreen: bool,
    /// WebSocket server port
    pub websocket_port: u16,
    /// Shared secret for mobile pairing
    pub ws_auth_token: Option<String>,
    /// Output folder for generated files
    pub output_folder: Option<String>,
    /// Use Tailscale for remote mobile access
    pub use_tailscale: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            startup_with_windows: false,
            minimise_to_tray: true,
            suppress_sleep_during_tasks: true,
            agents_active_on_lockscreen: true,
            websocket_port: 9001,
            ws_auth_token: None,
            output_folder: None,
            use_tailscale: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NetworkInfo
// ─────────────────────────────────────────────────────────────────────────────

/// Describes how the mobile app can reach Office Hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// LAN IPv4 address(es) of the host machine
    pub lan_ips: Vec<String>,
    /// Tailscale IPv4 address (if available)
    pub tailscale_ip: Option<String>,
    /// Tailscale DNS hostname (e.g. "my-pc.tail1234.ts.net")
    pub tailscale_hostname: Option<String>,
    /// WebSocket port
    pub ws_port: u16,
    /// Preferred connection address for QR code
    pub preferred_address: String,
    /// Timestamp of last probe
    pub probed_at: DateTime<Utc>,
}

impl NetworkInfo {
    pub fn probe(ts: &tailscale::TailscaleState, ws_port: u16) -> Self {
        let lan_ips = get_lan_ips();
        let tailscale_ip = ts.ip_v4.clone();
        let tailscale_hostname = ts.dns_hostname.clone();

        // Prefer Tailscale address (works across internet), then LAN
        let preferred_address = tailscale_ip
            .clone()
            .or_else(|| lan_ips.first().cloned())
            .unwrap_or_else(|| "127.0.0.1".to_string());

        Self {
            lan_ips,
            tailscale_ip,
            tailscale_hostname,
            ws_port,
            preferred_address,
            probed_at: Utc::now(),
        }
    }

    pub fn ws_url(&self) -> String {
        format!("ws://{}:{}", self.preferred_address, self.ws_port)
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "lanIps":            self.lan_ips,
            "tailscaleIp":       self.tailscale_ip,
            "tailscaleHostname": self.tailscale_hostname,
            "wsPort":            self.ws_port,
            "preferredAddress":  self.preferred_address,
            "wsUrl":             self.ws_url(),
            "probedAt":          self.probed_at.to_rfc3339(),
        })
    }
}

/// QR code payload sent to mobile during pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingQrPayload {
    /// The content encoded in the QR (JSON string)
    pub qr_data: String,
    /// SVG string of the QR code
    pub qr_svg: String,
    /// Parsed pairing info (for display in Settings UI)
    pub pairing_info: PairingInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    /// Primary SSE/REST URL (http://IP:9002)
    pub url: String,
    /// All available URLs (LAN + Tailscale)
    pub urls: Vec<String>,
    /// Auth token (if set)
    pub token: Option<String>,
    /// Expires at (QR codes expire after 5 minutes for security)
    pub expires_at: DateTime<Utc>,
    /// App version
    pub version: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// LAN IP detection
// ─────────────────────────────────────────────────────────────────────────────

fn get_lan_ips() -> Vec<String> {
    let mut ips = Vec::new();
    if let Ok(ip) = local_ip_address::local_ip() {
        ips.push(ip.to_string());
    }
    if ips.is_empty() {
        ips.push("127.0.0.1".to_string());
    }
    ips
}

// ─────────────────────────────────────────────────────────────────────────────
// sub-module: power
// ─────────────────────────────────────────────────────────────────────────────

pub mod power {
    //! Windows power management – prevent sleep during active tasks.
    //!
    //! Uses `SetThreadExecutionState()` from `kernel32` / `Win32_System_Power`.
    //!
    //! References:
    //!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate

    use tracing::info;

    #[cfg(windows)]
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_AWAYMODE_REQUIRED, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };

    /// Suppress system sleep and away-mode.
    ///
    /// Must be called from the same thread periodically, or once with `ES_CONTINUOUS`.
    /// Returns `true` on success.
    pub fn suppress_sleep() -> bool {
        #[cfg(windows)]
        unsafe {
            // ES_CONTINUOUS: keep in effect until called again
            // ES_SYSTEM_REQUIRED: prevent sleep
            // ES_AWAYMODE_REQUIRED: prevent away mode (screen saver / sleep)
            let flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED;
            let result = SetThreadExecutionState(flags);
            result.0 != 0
        }
        #[cfg(not(windows))]
        {
            warn!("suppress_sleep: no-op on non-Windows platform");
            false
        }
    }

    /// Re-allow system sleep.
    pub fn release_sleep() {
        #[cfg(windows)]
        unsafe {
            // ES_CONTINUOUS with no other flags → clear all previous requests
            SetThreadExecutionState(ES_CONTINUOUS);
            info!("SetThreadExecutionState(ES_CONTINUOUS) – sleep allowed again");
        }
        #[cfg(not(windows))]
        {
            warn!("release_sleep: no-op on non-Windows platform");
        }
    }

    /// Monitor power events (sleep / wake / lock / unlock).
    ///
    /// TODO(phase-1): Register WM_POWERBROADCAST handler in the Tauri window procedure.
    /// Events of interest:
    ///   PBT_APMSUSPEND     – system about to sleep
    ///   PBT_APMRESUMEAUTOMATIC – system woke from sleep
    ///   WTS_SESSION_LOCK   – user locked screen
    ///   WTS_SESSION_UNLOCK – user unlocked screen
    pub async fn monitor_power_events() {
        // TODO(phase-1): implement power event listener via WinAPI message loop
        // In the meantime, poll every 5 s and check if sleep should be re-asserted.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sub-module: startup
// ─────────────────────────────────────────────────────────────────────────────

pub mod startup {
    //! Register / unregister Office Hub in the Windows startup registry key.
    //!
    //! Key: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
    //! Value name: "OfficeHub"
    //! Value data: "<path to office-hub.exe>"

    use tracing::info;

    const REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const REG_VALUE: &str = "OfficeHub";

    #[cfg(windows)]
    pub fn register() -> anyhow::Result<()> {
        let exe_path = std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("Cannot determine exe path: {e}"))?;
        let exe_str = format!("\"{}\" --minimized", exe_path.display());

        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(REG_KEY)?;
        key.set_value(REG_VALUE, &exe_str)?;

        info!(exe = %exe_str, "Startup: registered");
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn register() -> anyhow::Result<()> {
        anyhow::bail!("Windows startup registration not supported on this platform")
    }

    #[cfg(windows)]
    pub fn unregister() -> anyhow::Result<()> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(REG_KEY, winreg::enums::KEY_SET_VALUE) {
            let _ = key.delete_value(REG_VALUE);
        }
        info!("Startup: unregistered");
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn unregister() -> anyhow::Result<()> {
        anyhow::bail!("Windows startup registration not supported on this platform")
    }

    #[cfg(windows)]
    pub fn is_registered() -> bool {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(REG_KEY) {
            let val: Result<String, _> = key.get_value(REG_VALUE);
            if let Ok(v) = val {
                if let Ok(exe_path) = std::env::current_exe() {
                    let exe_str = format!("\"{}\"", exe_path.display());
                    return v.contains(&exe_str);
                }
            }
        }
        false
    }

    #[cfg(not(windows))]
    pub fn is_registered() -> bool {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sub-module: tailscale
// ─────────────────────────────────────────────────────────────────────────────

pub mod tailscale {
    //! Tailscale integration – detect IP and DNS hostname for remote mobile access.
    //!
    //! Tailscale exposes a local JSON API at http://localhost:41112 or a CLI:
    //!   `tailscale status --json`
    //!
    //! We use the CLI output as it is the most stable interface across versions.

    use serde::{Deserialize, Serialize};
    use tracing::{debug, info, warn};

    /// Current Tailscale connectivity state.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct TailscaleState {
        /// Is Tailscale installed on this machine?
        pub installed: bool,
        /// Is the Tailscale daemon running?
        pub running: bool,
        /// Is the device currently connected to a Tailnet?
        pub connected: bool,
        /// Tailscale IPv4 address (e.g. "100.64.1.5")
        pub ip_v4: Option<String>,
        /// Tailscale IPv6 address
        pub ip_v6: Option<String>,
        /// Machine DNS name (e.g. "my-pc.tail1234.ts.net")
        pub dns_hostname: Option<String>,
        /// Tailnet name (e.g. "tail1234.ts.net")
        pub tailnet: Option<String>,
        /// Tailscale version
        pub version: Option<String>,
        /// Error message if detection failed
        pub error: Option<String>,
    }

    impl TailscaleState {
        pub fn as_json(&self) -> serde_json::Value {
            serde_json::json!({
                "installed":     self.installed,
                "running":       self.running,
                "connected":     self.connected,
                "ipV4":          self.ip_v4,
                "ipV6":          self.ip_v6,
                "dnsHostname":   self.dns_hostname,
                "tailnet":       self.tailnet,
                "version":       self.version,
                "error":         self.error,
            })
        }

        /// The preferred address to advertise in the QR code.
        /// Prefers DNS hostname (more stable) over raw IP.
        pub fn preferred_address(&self) -> Option<String> {
            self.dns_hostname.clone().or_else(|| self.ip_v4.clone())
        }
    }

    /// Probe Tailscale status by running `tailscale status --json`.
    pub async fn probe() -> TailscaleState {
        debug!("Probing Tailscale status…");

        // Check if tailscale binary is in PATH
        let which_result = tokio::process::Command::new("tailscale")
            .arg("version")
            .output()
            .await;

        if which_result.is_err() {
            debug!("Tailscale not found in PATH");
            return TailscaleState {
                installed: false,
                ..Default::default()
            };
        }

        let version_output = match which_result {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => String::new(),
        };

        // Run `tailscale status --json`
        let status_output = tokio::process::Command::new("tailscale")
            .args(["status", "--json"])
            .output()
            .await;

        match status_output {
            Ok(output) if output.status.success() => {
                let json_str = String::from_utf8_lossy(&output.stdout);
                parse_tailscale_json(&json_str, &version_output)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("tailscale status failed: {}", stderr);
                TailscaleState {
                    installed: true,
                    running: false,
                    error: Some(stderr.to_string()),
                    version: Some(version_output),
                    ..Default::default()
                }
            }
            Err(e) => {
                warn!("Failed to run tailscale CLI: {e}");
                TailscaleState {
                    installed: false,
                    error: Some(e.to_string()),
                    ..Default::default()
                }
            }
        }
    }

    pub fn parse_tailscale_json(json_str: &str, version: &str) -> TailscaleState {
        let json: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                return TailscaleState {
                    installed: true,
                    running: false,
                    error: Some(format!("JSON parse error: {e}")),
                    version: Some(version.to_string()),
                    ..Default::default()
                }
            }
        };

        // `tailscale status --json` schema (key fields):
        // {
        //   "BackendState": "Running" | "Stopped" | "NeedsLogin",
        //   "Self": {
        //     "DNSName": "my-pc.tail1234.ts.net.",
        //     "TailscaleIPs": ["100.64.1.5", "fd7a::1"],
        //     "HostName": "my-pc"
        //   },
        //   "MagicDNSSuffix": "tail1234.ts.net",
        //   "Version": "1.52.0"
        // }

        let backend_state = json["BackendState"].as_str().unwrap_or("Unknown");
        let running = backend_state == "Running";
        let connected = running;

        let self_node = &json["Self"];
        let dns_name_raw = self_node["DNSName"].as_str().unwrap_or("");
        // Strip trailing dot from DNS name
        let dns_hostname = if dns_name_raw.is_empty() {
            None
        } else {
            Some(dns_name_raw.trim_end_matches('.').to_string())
        };

        let ips: Vec<String> = self_node["TailscaleIPs"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let ip_v4 = ips.iter().find(|ip| !ip.contains(':')).cloned();
        let ip_v6 = ips.iter().find(|ip| ip.contains(':')).cloned();
        let tailnet = json["MagicDNSSuffix"].as_str().map(String::from);

        let ts_version = json["Version"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| version.to_string());

        if connected {
            info!(
                ip = ?ip_v4,
                hostname = ?dns_hostname,
                "Tailscale connected"
            );
        }

        TailscaleState {
            installed: true,
            running,
            connected,
            ip_v4,
            ip_v6,
            dns_hostname,
            tailnet,
            version: Some(ts_version),
            error: None,
        }
    }

    /// Install Tailscale instructions URL.
    pub const INSTALL_URL: &str = "https://tailscale.com/download/windows";
}

// ─────────────────────────────────────────────────────────────────────────────
// sub-module: qrcode
// ─────────────────────────────────────────────────────────────────────────────

pub mod qrcode {
    //! QR code generation for mobile pairing.
    //!
    //! The QR code encodes a JSON payload that the mobile app scans to
    //! automatically configure the WebSocket connection.
    //!
    //! Payload schema:
    //! {
    //!   "type":    "office-hub-pairing",
    //!   "version": "1",
    //!   "urls":    ["ws://192.168.1.10:9001", "ws://100.64.1.5:9001"],
    //!   "token":   "optional-auth-token",
    //!   "expires": "2025-01-01T12:05:00Z"   ← 5 minutes from generation
    //! }
    //!
    //! QR rendering: use the `qrcode` crate (pure Rust, no external deps).
    //! TODO(phase-1): Add `qrcode = "0.14"` to Cargo.toml.

    use chrono::Utc;
    use tracing::info;

    use super::{NetworkInfo, PairingInfo, PairingQrPayload};

    /// Generate a pairing QR code for the current network configuration.
    pub fn generate_pairing_qr(
        net: &NetworkInfo,
        auth_token: Option<&str>,
    ) -> anyhow::Result<PairingQrPayload> {
        let expires_at = Utc::now() + chrono::Duration::minutes(5);

        // Build list of all possible WebSocket URLs
        // Priority: LAN (fastest) → Tailscale IP (numeric, no DNS) → Tailscale hostname (DNS)
        let mut all_urls = Vec::new();

        // 1. LAN IPs — fastest, works on same Wi-Fi network
        for ip in &net.lan_ips {
            // FIX Bug #2: SSE server is on ws_port+1 (9002), use http:// not ws://
            let url = format!("http://{}:{}", ip, net.ws_port + 1);
            if !all_urls.contains(&url) {
                all_urls.push(url);
            }
        }

        // 2. Tailscale IP (numeric) — works remotely, no DNS lookup needed
        if let Some(ref ts_ip) = net.tailscale_ip {
            let url = format!("http://{}:{}", ts_ip, net.ws_port + 1);
            if !all_urls.contains(&url) {
                all_urls.push(url);
            }
        }

        // 3. Tailscale hostname — DNS-dependent, last resort
        if let Some(ref ts_host) = net.tailscale_hostname {
            let url = format!("http://{}:{}", ts_host, net.ws_port + 1);
            if !all_urls.contains(&url) {
                all_urls.push(url);
            }
        }

        // Primary URL (first in list = highest priority)
        let primary_url = all_urls
            .first()
            .cloned()
            .unwrap_or_else(|| format!("http://{}:{}", net.preferred_address, net.ws_port + 1));

        // Build JSON payload
        let qr_data = serde_json::json!({
            "type":    "office-hub-pairing",
            "version": "1",
            "urls":    all_urls.clone(),
            "token":   auth_token,
            "expires": expires_at.to_rfc3339(),
        })
        .to_string();

        // Generate QR SVG
        // In Phase 1 we send the JSON data to the frontend, which renders the SVG.
        let qr_svg = String::new();

        info!(
            primary_url = %primary_url,
            url_count   = all_urls.len(),
            expires_at  = %expires_at,
            "QR code generated for mobile pairing"
        );

        Ok(PairingQrPayload {
            qr_data,
            qr_svg,
            pairing_info: PairingInfo {
                url: primary_url,
                urls: all_urls,
                token: auth_token.map(String::from),
                expires_at,
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sub-module: tray
// ─────────────────────────────────────────────────────────────────────────────

pub mod tray {
    //! System tray icon and context menu management.
    //!
    //! Tauri v2 provides `tauri-plugin-positioner` and the built-in `TrayIcon` API.
    //!
    //! Menu items:
    //!   ✅ Open Office Hub       → show/focus main window
    //!   ─────────────────────
    //!   📊 Agent Status          → submenu: Analyst [idle], OfficeMaster [idle], …
    //!   ▶ Running workflows (N)  → show workflow panel
    //!   ─────────────────────
    //!   ⚙ Settings              → open Settings tab
    //!   📱 Mobile Pairing QR    → show QR code dialog
    //!   ─────────────────────
    //!   ❌ Quit                  → shutdown and exit

    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::{AppHandle, Emitter, Manager};
    use tracing::info;

    /// Build and register the system tray icon.
    ///
    /// Called once from `lib.rs`'s `setup()` callback.
    pub fn setup_tray(app: &AppHandle) -> anyhow::Result<()> {
        let quit = MenuItemBuilder::with_id("quit", "❌ Quit").build(app)?;
        let open = MenuItemBuilder::with_id("open", "✅ Open Office Hub").build(app)?;
        let qr = MenuItemBuilder::with_id("qr", "📱 Mobile Pairing QR").build(app)?;
        let settings = MenuItemBuilder::with_id("settings", "⚙ Settings").build(app)?;

        let menu = MenuBuilder::new(app)
            .item(&open)
            .separator()
            .item(&qr)
            .item(&settings)
            .separator()
            .item(&quit)
            .build()?;

        TrayIconBuilder::new()
            .icon(app.default_window_icon().unwrap().clone())
            .tooltip("Office Hub")
            .menu(&menu)
            .on_menu_event(|app, event| handle_tray_menu_event(app, event.id().as_ref()))
            .on_tray_icon_event(|tray, event| {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    // Left-click → show/focus window
                    if let Some(window) = tray.app_handle().get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            })
            .build(app)?;

        info!("System tray icon setup complete");
        Ok(())
    }

    /// Handle tray context menu events.
    fn handle_tray_menu_event(app: &AppHandle, event_id: &str) {
        match event_id {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "settings" => {
                // Emit event to frontend to navigate to Settings tab
                let _ = app.emit("navigate", serde_json::json!({ "tab": "settings" }));
            }
            "qr" => {
                let _ = app.emit("navigate", serde_json::json!({ "tab": "mobile-pairing" }));
            }
            "quit" => {
                info!("Quit requested from tray menu");
                app.exit(0);
            }
            other => {
                tracing::debug!("Unknown tray menu event: {}", other);
            }
        }
    }

    /// Update the tray tooltip to reflect current status.
    /// e.g. "Office Hub – 2 workflows running"
    pub fn update_tooltip(_app: &AppHandle, message: &str) {
        // TODO(phase-1): tray_icon.set_tooltip(message)
        tracing::debug!("Tray tooltip: {}", message);
    }

    /// Change the tray icon to indicate a notification (e.g. HITL pending).
    pub fn set_notification_badge(_app: &AppHandle, has_notification: bool) {
        // TODO(phase-1): swap icon between normal and "badge" variant
        tracing::debug!("Tray notification badge: {}", has_notification);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands (re-exported via commands.rs)
// ─────────────────────────────────────────────────────────────────────────────

/// All Tauri IPC commands related to system settings.
pub mod commands {
    use tauri::{AppHandle, State};

    use super::{PairingQrPayload, SystemConfig, SystemManager};

    pub type CmdResult<T> = Result<T, String>;
    fn e<E: std::fmt::Display>(err: E) -> String {
        err.to_string()
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    #[tauri::command]
    pub async fn get_system_config(mgr: State<'_, SystemManager>) -> CmdResult<SystemConfig> {
        Ok(mgr.config.read().await.clone())
    }

    #[tauri::command]
    pub async fn save_system_config(
        new_config: SystemConfig,
        mgr: State<'_, SystemManager>,
        _app: AppHandle,
    ) -> CmdResult<()> {
        let was_startup = mgr.config.read().await.startup_with_windows;

        *mgr.config.write().await = new_config.clone();

        // Apply startup registration change
        if new_config.startup_with_windows != was_startup {
            if new_config.startup_with_windows {
                super::startup::register().map_err(e)?;
            } else {
                super::startup::unregister().map_err(e)?;
            }
        }

        // Refresh network if Tailscale preference changed
        mgr.refresh_network().await;

        tracing::info!("System config saved");
        Ok(())
    }

    // ── Sleep ─────────────────────────────────────────────────────────────────

    #[tauri::command]
    pub async fn suppress_sleep(mgr: State<'_, SystemManager>) -> CmdResult<bool> {
        Ok(mgr.suppress_sleep())
    }

    #[tauri::command]
    pub async fn release_sleep(mgr: State<'_, SystemManager>) -> CmdResult<()> {
        mgr.release_sleep();
        Ok(())
    }

    // ── Startup ───────────────────────────────────────────────────────────────

    #[tauri::command]
    pub fn toggle_startup(enable: bool) -> CmdResult<()> {
        if enable {
            super::startup::register().map_err(e)
        } else {
            super::startup::unregister().map_err(e)
        }
    }

    #[tauri::command]
    pub fn get_startup_enabled() -> bool {
        super::startup::is_registered()
    }

    // ── QR Code / Pairing ─────────────────────────────────────────────────────

    #[tauri::command]
    pub async fn get_pairing_qr(mgr: State<'_, SystemManager>) -> CmdResult<PairingQrPayload> {
        mgr.generate_pairing_qr().await.map_err(e)
    }

    #[tauri::command]
    pub async fn get_network_info(mgr: State<'_, SystemManager>) -> CmdResult<serde_json::Value> {
        Ok(mgr.network_info.read().await.as_json())
    }

    #[tauri::command]
    pub async fn get_tailscale_status(
        mgr: State<'_, SystemManager>,
    ) -> CmdResult<serde_json::Value> {
        Ok(mgr.tailscale.read().await.as_json())
    }

    #[tauri::command]
    pub async fn refresh_network(mgr: State<'_, SystemManager>) -> CmdResult<serde_json::Value> {
        mgr.refresh_network().await;
        Ok(mgr.network_info.read().await.as_json())
    }

    // ── Status ────────────────────────────────────────────────────────────────

    #[tauri::command]
    pub async fn get_system_status(mgr: State<'_, SystemManager>) -> CmdResult<serde_json::Value> {
        Ok(mgr.status_json().await)
    }

    // ── Add-in Installer ──────────────────────────────────────────────────────

    #[tauri::command]
    pub async fn install_office_addin() -> CmdResult<()> {
        let mut current_dir = std::env::current_dir().map_err(e)?;

        // Nếu đang chạy ở chế độ dev, current_dir có thể là `src-tauri`
        if current_dir.ends_with("src-tauri") {
            if let Some(parent) = current_dir.parent() {
                current_dir = parent.to_path_buf();
            }
        }

        let addin_dir = current_dir.join("office-addin");
        let script_path = addin_dir.join("Setup-OfficeAddin.ps1");

        if !script_path.exists() {
            return Err(format!(
                "Setup-OfficeAddin.ps1 not found at {:?}",
                script_path
            ));
        }

        // Run the PowerShell script. It handles UAC elevation internally for the LocalMachine cert.
        let output = tokio::process::Command::new("powershell")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script_path.to_string_lossy().as_ref())
            .current_dir(&addin_dir)
            .output()
            .await
            .map_err(|err| format!("Failed to execute PowerShell script: {}", err))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            tracing::error!(
                "Add-in install failed. stdout: {}, stderr: {}",
                stdout,
                stderr
            );
            return Err(format!("Installation failed: {}", stderr));
        }

        tracing::info!("Office Add-in successfully installed via Settings");
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_system_config() {
        let cfg = SystemConfig::default();
        assert!(!cfg.startup_with_windows);
        assert!(cfg.minimise_to_tray);
        assert!(cfg.suppress_sleep_during_tasks);
        assert!(cfg.agents_active_on_lockscreen);
        assert_eq!(cfg.websocket_port, 9001);
    }

    #[test]
    fn test_get_lan_ips_returns_something() {
        let ips = get_lan_ips();
        assert!(!ips.is_empty());
    }

    #[tokio::test]
    async fn test_tailscale_probe_not_panics() {
        // Should not panic even if Tailscale is not installed
        let state = tailscale::probe().await;
        // We can't assert installed/not since it depends on the test environment,
        // but the function must return without panic.
        let _ = state.as_json();
    }

    #[test]
    fn test_tailscale_parse_not_running() {
        // Simulate a "BackendState: Stopped" response
        let json = r#"{
            "BackendState": "Stopped",
            "Self": {},
            "Version": "1.52.0"
        }"#;
        let state = tailscale::parse_tailscale_json(json, "1.52.0");
        assert!(state.installed);
        assert!(!state.running);
        assert!(!state.connected);
        assert!(state.ip_v4.is_none());
    }

    #[test]
    fn test_tailscale_parse_running() {
        let json = r#"{
            "BackendState": "Running",
            "Self": {
                "DNSName": "my-pc.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.1.5", "fd7a::1234"],
                "HostName": "my-pc"
            },
            "MagicDNSSuffix": "tail1234.ts.net",
            "Version": "1.52.1"
        }"#;
        let state = tailscale::parse_tailscale_json(json, "1.52.1");
        assert!(state.installed);
        assert!(state.running);
        assert!(state.connected);
        assert_eq!(state.ip_v4.as_deref(), Some("100.64.1.5"));
        assert_eq!(state.ip_v6.as_deref(), Some("fd7a::1234"));
        // DNS name should have trailing dot stripped
        assert_eq!(state.dns_hostname.as_deref(), Some("my-pc.tail1234.ts.net"));
        assert_eq!(state.tailnet.as_deref(), Some("tail1234.ts.net"));
    }

    #[test]
    fn test_network_info_ws_url() {
        let ts = tailscale::TailscaleState {
            ip_v4: Some("100.64.1.5".to_string()),
            ..Default::default()
        };
        let net = NetworkInfo::probe(&ts, 9001);
        // Tailscale IP should be preferred
        assert_eq!(net.preferred_address, "100.64.1.5");
        assert_eq!(net.ws_url(), "ws://100.64.1.5:9001");
    }

    #[test]
    fn test_qr_generate_stub() {
        let ts = tailscale::TailscaleState::default();
        let net = NetworkInfo::probe(&ts, 9001);
        let result = qrcode::generate_pairing_qr(&net, Some("test-token"));
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.qr_data.contains("office-hub-pairing"));
        assert!(payload.qr_data.contains("test-token"));
        // qr_svg is not generated yet (it's a stub String::new() returning empty)
        assert!(payload.qr_svg.is_empty());
        assert!(!payload.pairing_info.urls.is_empty());
        assert!(payload.pairing_info.expires_at > chrono::Utc::now());
    }

    #[test]
    fn test_qr_without_token() {
        let ts = tailscale::TailscaleState::default();
        let net = NetworkInfo::probe(&ts, 9001);
        let result = qrcode::generate_pairing_qr(&net, None);
        assert!(result.is_ok());
        // token field should be null in the JSON
        let payload = result.unwrap();
        assert!(payload.pairing_info.token.is_none());
    }

    #[test]
    fn test_startup_is_registered_does_not_panic() {
        // Must not panic on any platform
        let _ = startup::is_registered();
    }

    #[test]
    fn test_tailscale_preferred_address() {
        let mut ts = tailscale::TailscaleState::default();
        ts.dns_hostname = Some("my-pc.ts.net".to_string());
        ts.ip_v4 = Some("100.64.1.5".to_string());
        // DNS hostname preferred over raw IP
        assert_eq!(ts.preferred_address().as_deref(), Some("my-pc.ts.net"));

        ts.dns_hostname = None;
        assert_eq!(ts.preferred_address().as_deref(), Some("100.64.1.5"));

        ts.ip_v4 = None;
        assert!(ts.preferred_address().is_none());
    }
}
