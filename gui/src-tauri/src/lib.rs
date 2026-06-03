use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::net::TcpStream;
use std::sync::Mutex;
use std::os::windows::process::CommandExt;
use tokio::sync::oneshot;

mod proxy;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// User-writable data directory: %APPDATA%\Anthropic Provider Gateway Manager
fn user_data_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| {
        std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
    });
    PathBuf::from(appdata).join("Anthropic Provider Gateway Manager")
}

/// Find the bundled config.json shipped with the app (read-only template).
fn find_bundled_config() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for candidate in &[parent.to_path_buf(), parent.join("resources")] {
                let config = candidate.join("config.json");
                if config.exists() {
                    return Some(config);
                }
            }
        }
    }
    // Dev mode fallback
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_config = manifest.join("..").join("..").join("config.json");
    if dev_config.exists() {
        return Some(dev_config);
    }
    None
}

/// Returns the path to the user-writable config.json.
/// Seeds from bundled config on first run if no user copy exists.
fn config_path() -> PathBuf {
    let dir = user_data_dir();
    let user_config = dir.join("config.json");

    if !user_config.exists() {
        let _ = std::fs::create_dir_all(&dir);
        if let Some(bundled) = find_bundled_config() {
            let _ = std::fs::copy(&bundled, &user_config);
        }
    }

    user_config
}

fn log_dir() -> PathBuf {
    user_data_dir().join("Communication-Logs")
}


// ---------------------------------------------------------------------------
// Command 1: Health check
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    upstream: String,
}

#[tauri::command]
async fn check_health() -> Result<HealthResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    match client
        .get("http://127.0.0.1:4000/health")
        .send()
        .await
    {
        Ok(resp) => {
            let json: serde_json::Value =
                resp.json().await.map_err(|e| e.to_string())?;
            Ok(HealthResponse {
                status: json["status"]
                    .as_str()
                    .unwrap_or("unknown")
                    .into(),
                upstream: json["upstream"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
            })
        }
        Err(_) => Ok(HealthResponse {
            status: "unreachable".into(),
            upstream: "".into(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Command 1b: Gateway status (used by dashboard)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct GatewayStatusResponse {
    reachable: bool,
    port_listening: bool,
    checked_at: String,
    error: Option<String>,
    managed_child_running: bool,
    managed_child_pid: Option<u32>,
    diagnostic: String,
}

#[tauri::command]
fn check_gateway_status(state: tauri::State<'_, ProxyState>) -> GatewayStatusResponse {
    use std::net::TcpStream;
    use std::time::Duration;

    let now = Local::now();
    let checked_at = now.format("%Y-%m-%d %H:%M:%S").to_string();

    // Check managed axum task
    let (managed_child_running, managed_child_pid) = {
        let guard = match state.handle.lock() {
            Ok(g) => g,
            Err(_) => {
                return GatewayStatusResponse {
                    reachable: false,
                    port_listening: false,
                    checked_at,
                    error: Some("Cannot lock proxy state".into()),
                    managed_child_running: false,
                    managed_child_pid: None,
                    diagnostic: "Lock error".into(),
                };
            }
        };
        match &*guard {
            Some(handle) => (!handle.inner().is_finished(), None),
            None => (false, None),
        }
    };

    // Check TCP port 4000
    let port_reachable = TcpStream::connect_timeout(
        &"127.0.0.1:4000".parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok();

    let port_listening = port_reachable;

    let diagnostic = format!(
        "managed_child_running: {}, managed_child_pid: {:?}, port_reachable: {}",
        managed_child_running, managed_child_pid, port_reachable
    );

    GatewayStatusResponse {
        reachable: port_reachable,
        port_listening,
        checked_at,
        error: None,
        managed_child_running,
        managed_child_pid,
        diagnostic,
    }
}

// ---------------------------------------------------------------------------
// Command 2: Check API key
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ApiKeyStatus {
    set: bool,
    env_var: String,
}

#[tauri::command]
fn check_api_key() -> Result<ApiKeyStatus, String> {
    match get_active_api_key_env() {
        Ok(env_var) => {
            let set = std::env::var(&env_var).is_ok();
            Ok(ApiKeyStatus { set, env_var })
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Command 3: Set API key as environment variable
// ---------------------------------------------------------------------------

#[tauri::command]
fn set_env_api_key(key: String, env_var_name: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();

    // Persist to user environment variable (survives app restart)
    // setx doesn't affect the current process, so we also call set_var below
    let output = std::process::Command::new("setx")
        .args([&env_var_name, &trimmed])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("Failed to run setx: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("setx failed: {}", stderr));
    }

    // Also set for current process (setx only affects new processes)
    std::env::set_var(&env_var_name, &trimmed);

    Ok(())
}

// ---------------------------------------------------------------------------
// Command 3x: Update provider's api_key_env in config.json
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_provider_api_key_env(provider_id: String, api_key_env: String) -> Result<(), String> {
    // Validate env var name format: uppercase letters, digits, underscores only
    if api_key_env.is_empty() {
        return Err("Environment variable name cannot be empty".into());
    }
    let valid = api_key_env
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !valid {
        return Err(
            "Environment variable name must be uppercase letters, digits, or underscores (e.g. MOONSHOT_API_KEY)"
                .into(),
        );
    }

    let path = config_path();
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;

    // Detect encoding
    let (encoding, mut cfg) = match String::from_utf8(bytes.clone()) {
        Ok(s) => ("UTF-8", serde_json::from_str::<serde_json::Value>(&s)
            .map_err(|e| format!("Invalid JSON: {}", e))?),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                return Err("Cannot decode config.json".into());
            }
            ("Shift-JIS", serde_json::from_str::<serde_json::Value>(&decoded.into_owned())
                .map_err(|e| format!("Invalid JSON: {}", e))?)
        }
    };

    // Update providers[provider_id].api_key_env
    let providers = cfg["providers"]
        .as_object_mut()
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get_mut(&provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;
    provider["api_key_env"] = serde_json::Value::String(api_key_env);

    // Write back preserving encoding
    let json_str = serde_json::to_string_pretty(&cfg).map_err(|e| format!("JSON error: {}", e))?;
    let output = match encoding {
        "Shift-JIS" => {
            let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode(&json_str);
            if had_errors {
                return Err("Cannot encode config as Shift-JIS".into());
            }
            encoded.into_owned()
        }
        _ => json_str.into_bytes(),
    };
    std::fs::write(&path, &output).map_err(|e| format!("Cannot write config.json: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 17: Check all API keys
// ---------------------------------------------------------------------------

#[tauri::command]
fn check_all_api_keys() -> Result<std::collections::HashMap<String, ApiKeyStatus>, String> {
    let cfg = load_gateway_config()?;
    let mut result = std::collections::HashMap::new();
    for (provider_id, provider) in &cfg.providers {
        let set = std::env::var(&provider.api_key_env).is_ok();
        result.insert(
            provider_id.clone(),
            ApiKeyStatus {
                set,
                env_var: provider.api_key_env.clone(),
            },
        );
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Command 18: Update active provider
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_active_provider(provider_id: String) -> Result<(), String> {
    let path = config_path();
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;

    // Detect encoding
    let (encoding, mut cfg) = match String::from_utf8(bytes.clone()) {
        Ok(s) => ("UTF-8", serde_json::from_str::<serde_json::Value>(&s)
            .map_err(|e| format!("Invalid JSON: {}", e))?),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                return Err("Cannot decode config.json".into());
            }
            ("Shift-JIS", serde_json::from_str::<serde_json::Value>(&decoded.into_owned())
                .map_err(|e| format!("Invalid JSON: {}", e))?)
        }
    };

    // Validate provider exists
    cfg["providers"]
        .as_object()
        .and_then(|p| p.get(&provider_id))
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;

    // Update active_provider
    cfg["active_provider"] = serde_json::Value::String(provider_id);

    // Write back preserving encoding
    let json_str = serde_json::to_string_pretty(&cfg).map_err(|e| format!("JSON error: {}", e))?;
    let output = match encoding {
        "Shift-JIS" => {
            let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode(&json_str);
            if had_errors {
                return Err("Cannot encode config as Shift-JIS".into());
            }
            encoded.into_owned()
        }
        _ => json_str.into_bytes(),
    };
    std::fs::write(&path, &output).map_err(|e| format!("Cannot write config.json: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 3b: Port 4000 process
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct PortProcessInfo {
    pid: String,
    raw_output: String,
}

#[tauri::command]
fn get_port_4000_process() -> Result<PortProcessInfo, String> {
    let output = std::process::Command::new("cmd")
        .args(["/C", "netstat -ano | findstr :4000"])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Extract PID from LISTENING line (5th whitespace-delimited token)
    let pid = stdout
        .lines()
        .find(|line| line.to_uppercase().contains("LISTENING"))
        .and_then(|line| {
            line.split_whitespace().nth(4).map(|s| s.to_string())
        })
        .unwrap_or_default();

    Ok(PortProcessInfo {
        pid,
        raw_output: stdout,
    })
}

// ---------------------------------------------------------------------------
// Command 4: Read config
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub display_name: String,
    pub upstream_url: String,
    pub api_key_env: String,
    pub default_model: String,
    pub force_anthropic_version: Option<String>,
    pub supports_count_tokens: bool,
    pub supports_vision: bool,
    pub supports_video: bool,
    pub supports_thinking: bool,
    pub model_map: std::collections::HashMap<String, String>,
    pub visible_models: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
}

#[derive(Serialize, Deserialize)]
pub struct GatewayConfigResponse {
    pub active_provider: String,
    pub providers: std::collections::HashMap<String, ProviderConfig>,
    pub server: ServerConfig,
}

#[tauri::command]
fn read_config() -> Result<GatewayConfigResponse, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read config.json: {}", e))?;
    let cfg: GatewayConfigResponse =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    Ok(cfg)
}

/// Load config (internal helper, returns parsed struct).
fn load_gateway_config() -> Result<GatewayConfigResponse, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read config.json: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Get the active provider's API key env var name from config.
fn get_active_api_key_env() -> Result<String, String> {
    let cfg = load_gateway_config()?;
    let provider = cfg.providers.get(&cfg.active_provider)
        .ok_or_else(|| format!("Active provider '{}' not found in config", cfg.active_provider))?;
    Ok(provider.api_key_env.clone())
}

// ---------------------------------------------------------------------------
// Command 5: Read latest log
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct LogFile {
    filename: String,
    content: String,
    line_count: usize,
}

#[tauri::command]
fn read_latest_log() -> Result<LogFile, String> {
    let dir = log_dir();

    if !dir.exists() {
        return Ok(LogFile {
            filename: String::new(),
            content: String::new(),
            line_count: 0,
        });
    }

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read log dir: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("proxy-") && name.ends_with(".log")
        })
        .collect();

    // Sort by filename descending (ISO dates = chronological order)
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    let latest = match entries.first() {
        Some(entry) => entry,
        None => {
            return Ok(LogFile {
                filename: String::new(),
                content: String::new(),
                line_count: 0,
            });
        }
    };

    let filename = latest.file_name().to_string_lossy().to_string();
    let bytes =
        std::fs::read(latest.path()).map_err(|e| format!("Cannot read log file: {}", e))?;

    // Try UTF-8 first, then fall back to Shift-JIS (for Japanese Windows)
    let content = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                String::from_utf8_lossy(&bytes).to_string()
            } else {
                decoded.into_owned()
            }
        }
    };
    let line_count = content.lines().count();

    Ok(LogFile {
        filename,
        content,
        line_count,
    })
}

// ---------------------------------------------------------------------------
// Command 6: Open logs folder in Explorer
// ---------------------------------------------------------------------------

#[tauri::command]
fn open_logs_folder() -> Result<(), String> {
    let dir = log_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create log dir: {}", e))?;
    }
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Cannot open folder: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 7: Open any path in Explorer
// ---------------------------------------------------------------------------

fn expand_env_vars(path: &str) -> String {
    let mut result = path.to_string();
    let mut start = 0;
    while let Some(pct) = result[start..].find('%') {
        let abs = start + pct;
        if let Some(end) = result[abs + 1..].find('%') {
            let var_name = &result[abs + 1..abs + 1 + end];
            if let Ok(val) = std::env::var(var_name) {
                result.replace_range(abs..abs + end + 2, &val);
                start = abs + val.len();
            } else {
                start = abs + end + 2;
            }
        } else {
            break;
        }
    }
    result
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let resolved = expand_env_vars(&path);
    std::process::Command::new("explorer")
        .arg(&resolved)
        .spawn()
        .map_err(|e| format!("Cannot open path: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 8: Read config raw (with encoding detection)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RawConfigResponse {
    content: String,
    encoding_used: String,
    config_path: String,
}

#[tauri::command]
fn read_config_raw() -> Result<RawConfigResponse, String> {
    let path = config_path();
    let config_path_str = path.to_string_lossy().to_string();
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;

    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(RawConfigResponse {
            content: s,
            encoding_used: "UTF-8".into(),
            config_path: config_path_str,
        }),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                Err("Cannot decode config.json as UTF-8 or Shift-JIS".into())
            } else {
                Ok(RawConfigResponse {
                    content: decoded.into_owned(),
                    encoding_used: "Shift-JIS".into(),
                    config_path: config_path_str,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command 9: Write config
// ---------------------------------------------------------------------------

#[tauri::command]
fn write_config(content: String, encoding: String) -> Result<(), String> {
    let path = config_path();
    let bytes: Vec<u8> = match encoding.as_str() {
        "Shift-JIS" => {
            let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode(&content);
            if had_errors {
                return Err("Cannot encode content as Shift-JIS".into());
            }
            encoded.into_owned()
        }
        _ => content.into_bytes(),
    };
    std::fs::write(&path, &bytes).map_err(|e| format!("Cannot write config.json: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 13: Find Claude Desktop config files
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ClaudeConfigCandidate {
    path: String,
    exists: bool,
    likely_config: bool,
}

#[tauri::command]
fn find_claude_configs() -> Result<Vec<ClaudeConfigCandidate>, String> {
    let mut candidates: Vec<ClaudeConfigCandidate> = Vec::new();

    // Build search directories from environment variables
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    let vars: &[(&str, &str)] = &[
        ("APPDATA", "Claude"),
        ("LOCALAPPDATA", "Claude"),
        ("LOCALAPPDATA", "Claude-3p\\configLibrary"),
        ("USERPROFILE", ".claude"),
    ];

    for (env_var, subdir) in vars {
        if let Ok(base) = std::env::var(env_var) {
            let dir = PathBuf::from(&base).join(subdir);
            if seen.insert(dir.clone()) {
                dirs.push(dir);
            }
        }
    }

    // Claude-specific keys that indicate a real config file
    let claude_keys = [
        "inferenceProvider",
        "claude_desktop_config",
        "inferenceGatewayBaseUrl",
        "inferenceModels",
        "inferenceGatewayApiKey",
    ];

    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.ends_with(".json") {
                // Check if file content suggests it's a Claude config
                let likely_config = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(|content| {
                        claude_keys
                            .iter()
                            .any(|key| content.contains(key))
                    })
                    .unwrap_or(false);

                candidates.push(ClaudeConfigCandidate {
                    path: path.to_string_lossy().to_string(),
                    exists: true,
                    likely_config,
                });
            }
        }
    }

    // Sort: likely configs first, then by path
    candidates.sort_by(|a, b| {
        b.likely_config
            .cmp(&a.likely_config)
            .then(a.path.cmp(&b.path))
    });
    candidates.dedup_by(|a, b| a.path == b.path);
    Ok(candidates)
}

// ---------------------------------------------------------------------------
// Command 14: List log files
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct LogListEntry {
    filename: String,
    size: u64,
}

#[tauri::command]
fn list_logs() -> Result<Vec<LogListEntry>, String> {
    let dir = log_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<LogListEntry> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read log dir: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("proxy-") && name.ends_with(".log")
        })
        .map(|e| {
            let filename = e.file_name().to_string_lossy().to_string();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            LogListEntry { filename, size }
        })
        .collect();

    entries.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Command 15: Read a specific log file
// ---------------------------------------------------------------------------

#[tauri::command]
fn read_log(filename: String) -> Result<LogFile, String> {
    let dir = log_dir();
    let path = dir.join(&filename);

    // Security: ensure the resolved path stays inside log_dir
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| format!("Cannot resolve log dir: {}", e))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|_| format!("Log file not found: {}", filename))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("Invalid log filename".into());
    }

    let bytes =
        std::fs::read(&canonical_path).map_err(|e| format!("Cannot read log file: {}", e))?;

    let content = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                String::from_utf8_lossy(&bytes).to_string()
            } else {
                decoded.into_owned()
            }
        }
    };
    let line_count = content.lines().count();

    Ok(LogFile {
        filename,
        content,
        line_count,
    })
}

// ---------------------------------------------------------------------------
// Command 16: Create new log file
// ---------------------------------------------------------------------------

#[tauri::command]
fn create_new_log() -> Result<String, String> {
    let dir = log_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create log dir: {}", e))?;
    }

    let now = Local::now();
    let filename = format!("proxy-{}.log", now.format("%Y%m%d-%H%M%S"));
    let path = dir.join(&filename);

    std::fs::write(&path, "").map_err(|e| format!("Cannot create log file: {}", e))?;
    Ok(filename)
}


// ---------------------------------------------------------------------------
// Proxy state
// ---------------------------------------------------------------------------

pub struct ProxyState {
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    done_rx: Mutex<Option<std::sync::mpsc::Receiver<Result<(), String>>>>,
}

impl ProxyState {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
            done_rx: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Command 10: Start proxy
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct StartProxyResult {
    success: bool,
    pid: u32,
    python: String,
    dir: String,
    log: String,
}

#[tauri::command]
fn start_proxy(state: tauri::State<'_, ProxyState>) -> Result<StartProxyResult, String> {
    let mut diag: Vec<String> = Vec::new();

    // --- Phase 1: Check/clear previous state (brief lock) ---
    {
        let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
        let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;

        if let Some(ref handle) = *handle_guard {
            if !handle.inner().is_finished() {
                return Ok(StartProxyResult {
                    success: false,
                    pid: 0,
                    python: "rust-axum".into(),
                    dir: String::new(),
                    log: "already_running".into(),
                });
            }
            *handle_guard = None;
            *shutdown_guard = None;
            *done_guard = None;
        }
    } // locks dropped

    // --- Phase 2: Load config and resolve proxy config (no locks held) ---
    let cfg = match load_gateway_config() {
        Ok(c) => c,
        Err(e) => return Err(format!("Cannot read config: {}", e)),
    };

    let api_key_env = cfg
        .providers
        .get(&cfg.active_provider)
        .map(|p| p.api_key_env.clone())
        .unwrap_or_default();
    diag.push(format!("Active provider API key env: {}", api_key_env));

    if std::env::var(&api_key_env).is_err() {
        diag.push(format!("{}: NOT SET", api_key_env));
        return Err(format!(
            "{} not set — set it in the API Key tab first.",
            api_key_env
        ));
    }
    diag.push(format!("{}: set", api_key_env));

    let proxy_config = match proxy::resolve_proxy_config(&cfg) {
        Ok(c) => {
            diag.push(format!("Upstream: {}", c.upstream_url));
            diag.push(format!("Provider: {} ({})", c.display_name, c.active_provider));
            c
        }
        Err(e) => return Err(format!("Config error: {}", e)),
    };

    let host = proxy_config.server_host.clone();
    let port = proxy_config.server_port;
    diag.push(format!("Starting proxy on {}:{}", host, port));

    let (tx, rx) = oneshot::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let handle = tauri::async_runtime::spawn(async move {
        let result = proxy::run_proxy_server(host, port, proxy_config, rx).await;
        let _ = done_tx.send(
            result.map_err(|e| e.to_string())
        );
    });

    // --- Phase 3: Store handle, shutdown sender, and done receiver (brief lock) ---
    {
        let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
        let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;
        *handle_guard = Some(handle);
        *shutdown_guard = Some(tx);
        *done_guard = Some(done_rx);
    } // locks dropped

    // --- Phase 4: Poll for port reachability (no locks held) ---
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if TcpStream::connect_timeout(
            &"127.0.0.1:4000".parse().unwrap(),
            std::time::Duration::from_millis(200),
        )
        .is_ok()
        {
            diag.push(format!(
                "Port 4000 reachable after {:.1}s",
                start.elapsed().as_secs_f32()
            ));
            break;
        }
        if start.elapsed() >= timeout {
            // Re-acquire locks briefly just to clear state on failure
            let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
            let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
            let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;
            let _ = shutdown_guard.take().map(|tx| tx.send(()));
            let _ = handle_guard.take();
            let _ = done_guard.take();
            return Err(format!(
                "Proxy did not become reachable within {}s",
                timeout.as_secs()
            ));
        }
    }

    Ok(StartProxyResult {
        success: true,
        pid: 0,
        python: "rust-axum".into(),
        dir: String::new(),
        log: diag.join("\n"),
    })
}

// ---------------------------------------------------------------------------
// Command 11: Stop proxy
// ---------------------------------------------------------------------------

#[tauri::command]
fn stop_proxy(state: tauri::State<'_, ProxyState>) -> Result<String, String> {
    let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
    let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
    let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;

    let mut diag_parts: Vec<String> = Vec::new();

    // Send shutdown signal
    if let Some(tx) = shutdown_guard.take() {
        let _ = tx.send(());
        diag_parts.push("Shutdown signal sent".into());
    } else {
        diag_parts.push("No active shutdown channel".into());
    }

    // Wait for task to finish via mpsc channel (avoids block_on re-entrancy panic)
    if let Some(rx) = done_guard.take() {
        diag_parts.push("Waiting for proxy task to finish...".into());
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => diag_parts.push("Proxy task finished cleanly".into()),
            Ok(Err(e)) => diag_parts.push(format!("Proxy task error: {}", e)),
            Err(_) => diag_parts.push("Timeout waiting for proxy task to finish".into()),
        }
    } else {
        diag_parts.push("No active done channel".into());
    }

    // Clear handle
    let _ = handle_guard.take();

    // Check port 4000 after stopping
    let port_reachable = TcpStream::connect_timeout(
        &"127.0.0.1:4000".parse().unwrap(),
        std::time::Duration::from_millis(500),
    )
    .is_ok();
    diag_parts.push(format!("Port 4000 reachable after stop: {}", port_reachable));

    Ok(diag_parts.join("\n"))
}

// ---------------------------------------------------------------------------
// Command 12: Proxy status
// ---------------------------------------------------------------------------

#[tauri::command]
fn proxy_status(state: tauri::State<'_, ProxyState>) -> Result<bool, String> {
    let guard = state.handle.lock().map_err(|e| e.to_string())?;
    if let Some(ref handle) = *guard {
        Ok(!handle.inner().is_finished())
    } else {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ProxyState::new())
        .invoke_handler(tauri::generate_handler![
            check_health,
            check_gateway_status,
            check_api_key,
            get_port_4000_process,
            read_config,
            read_latest_log,
            open_logs_folder,
            open_path,
            read_config_raw,
            write_config,
            find_claude_configs,
            list_logs,
            read_log,
            create_new_log,
            set_env_api_key,
            update_provider_api_key_env,
            check_all_api_keys,
            update_active_provider,
            start_proxy,
            stop_proxy,
            proxy_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
