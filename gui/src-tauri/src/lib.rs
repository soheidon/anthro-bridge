use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::net::TcpStream;
use std::sync::Mutex;
use std::io::{BufRead, BufReader};
use std::fs::File;
use std::os::windows::process::CommandExt;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn project_root() -> PathBuf {
    // Prefer the EXE's parent directory (so the release/ folder works standalone).
    // Fall back to CARGO_MANIFEST_DIR/../.. for dev mode (cargo run / tauri dev).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let proxy = parent.join("proxy_server.py");
            if proxy.exists() {
                return parent.to_path_buf();
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("..").join("..")
}

fn log_dir() -> PathBuf {
    project_root().join("Communication-Logs")
}

fn config_path() -> PathBuf {
    project_root().join("config.json")
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

    // Check managed child
    let (managed_child_running, managed_child_pid) = {
        let mut guard = match state.child.lock() {
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
        match &mut *guard {
            Some(child) => {
                match child.try_wait() {
                    Ok(Some(_)) => (false, None),
                    Ok(None) => (true, Some(child.id())),
                    Err(_) => (false, None),
                }
            }
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
}

#[tauri::command]
fn read_config_raw() -> Result<RawConfigResponse, String> {
    let path = config_path();
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;

    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(RawConfigResponse {
            content: s,
            encoding_used: "UTF-8".into(),
        }),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                Err("Cannot decode config.json as UTF-8 or Shift-JIS".into())
            } else {
                Ok(RawConfigResponse {
                    content: decoded.into_owned(),
                    encoding_used: "Shift-JIS".into(),
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
    child: Mutex<Option<std::process::Child>>,
}

impl ProxyState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: read last N lines of a file
// ---------------------------------------------------------------------------

fn read_last_lines(path: &std::path::Path, n: usize) -> Vec<String> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    let start = if lines.len() > n { lines.len() - n } else { 0 };
    lines[start..].to_vec()
}

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

    let mut guard = state.child.lock().map_err(|e| e.to_string())?;

    if let Some(ref mut child) = *guard {
        match child.try_wait() {
            Ok(Some(_)) => *guard = None,
            Ok(None) => return Ok(StartProxyResult {
                success: false, pid: 0,
                python: String::new(), dir: String::new(),
                log: "already_running".into(),
            }),
            Err(e) => return Err(format!("Cannot check child status: {}", e)),
        }
    }

    // Read config to determine which API key env var the active provider needs
    let api_key_env = match get_active_api_key_env() {
        Ok(env) => {
            diag.push(format!("Active provider API key env: {}", env));
            env
        }
        Err(e) => {
            diag.push(format!("Cannot read active provider from config: {}", e));
            return Err(format!("Cannot read config: {}", e));
        }
    };

    let api_key_value = match std::env::var(&api_key_env) {
        Ok(k) => {
            diag.push(format!("{}: set (len={})", api_key_env, k.len()));
            k
        }
        Err(_) => {
            diag.push(format!("{}: NOT SET", api_key_env));
            return Err(format!("{} not set — set it in the API Key tab first.", api_key_env));
        }
    };

    let root = project_root();
    diag.push(format!("project_root: {}", root.display()));

    let proxy_py = root.join("proxy_server.py");
    diag.push(format!("proxy_server.py exists: {}", proxy_py.exists()));

    let config_json = root.join("config.json");
    diag.push(format!("config.json exists: {}", config_json.exists()));

    // Resolve python.exe via cmd so PATH matches the user's normal shell
    let python = std::process::Command::new("cmd")
        .args(["/C", "where python 2>nul"])
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.lines().next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty());
    diag.push(format!("where python result: {:?}", python));

    let python = python
        .ok_or_else(|| format!("Python not found. Diagnostics:\n{}", diag.join("\n")))?;

    diag.push(format!("Using python: {}", python));
    diag.push(format!("Launching: {} proxy_server.py in {}", python, root.display()));

    // stdout/stderr → file redirection (not piped) to avoid pipe buffer blocking uvicorn
    let logs_dir = root.join("Communication-Logs");
    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Cannot create log dir: {}", e))?;
    let uvicorn_log_path = logs_dir.join("uvicorn-stdout-stderr.log");
    diag.push(format!("uvicorn log: {}", uvicorn_log_path.display()));

    // Write startup marker before spawning
    {
        use std::io::Write;
        let now = Local::now();
        let marker = format!(
            "===== Starting proxy from GUI at {} =====\n\
             project_root: {}\n\
             python: {}\n\
             command: {} proxy_server.py\n\
             =====\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            root.display(),
            python,
            python
        );
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&uvicorn_log_path)
        {
            let _ = f.write_all(marker.as_bytes());
        }
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&uvicorn_log_path)
        .map_err(|e| format!("Cannot open uvicorn log file: {}", e))?;
    let err_file = log_file
        .try_clone()
        .map_err(|e| format!("Cannot clone uvicorn log file handle: {}", e))?;

    let mut child = std::process::Command::new(&python)
        .arg("proxy_server.py")
        .current_dir(&root)
        .env(&api_key_env, &api_key_value)
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(err_file))
        .spawn()
        .map_err(|e| format!("Cannot start proxy: {}\nDiagnostics:\n{}", e, diag.join("\n")))?;

    let pid = child.id();
    diag.push(format!("Spawned PID: {}", pid));

    // Poll port 4000 until reachable or timeout (8s, 300ms intervals)
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(8);
    let mut port_ok = false;

    loop {
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Check if process died
        match child.try_wait() {
            Ok(Some(status)) => {
                diag.push(format!("Process exited with code {:?} after {:.1}s", status.code(), start.elapsed().as_secs_f32()));
                diag.push(format!("See uvicorn log: {}", uvicorn_log_path.display()));
                // Read last 80 lines of uvicorn log
                let tail = read_last_lines(&uvicorn_log_path, 80);
                if !tail.is_empty() {
                    diag.push("--- last 80 lines of uvicorn log ---".into());
                    diag.extend(tail);
                    diag.push("--- end of uvicorn log ---".into());
                }
                return Err(format!("Proxy exited during startup. Diagnostics:\n{}", diag.join("\n")));
            }
            Err(e) => {
                return Err(format!("Cannot check proxy status: {}", e));
            }
            Ok(None) => {}
        }

        // Try TCP connect to 127.0.0.1:4000
        match TcpStream::connect_timeout(
            &"127.0.0.1:4000".parse().unwrap(),
            std::time::Duration::from_millis(200),
        ) {
            Ok(_) => {
                port_ok = true;
                diag.push(format!("Port 4000 reachable after {:.1}s", start.elapsed().as_secs_f32()));
                break;
            }
            Err(_) => {}
        }

        if start.elapsed() >= timeout {
            break;
        }
    }

    if !port_ok {
        diag.push(format!("Port 4000 did not become reachable within {}s", timeout.as_secs()));
        diag.push(format!("See uvicorn log: {}", uvicorn_log_path.display()));
        // Read last 80 lines of uvicorn log
        let tail = read_last_lines(&uvicorn_log_path, 80);
        if !tail.is_empty() {
            diag.push("--- last 80 lines of uvicorn log ---".into());
            diag.extend(tail);
            diag.push("--- end of uvicorn log ---".into());
        }
        // Kill the process since it's not working
        let _ = child.kill();
        let _ = child.wait();
        *guard = None;
        return Err(format!("Proxy process started but port 4000 did not become reachable. Diagnostics:\n{}", diag.join("\n")));
    }

    *guard = Some(child);
    Ok(StartProxyResult {
        success: true,
        pid,
        python,
        dir: root.display().to_string(),
        log: diag.join("\n"),
    })
}

// ---------------------------------------------------------------------------
// Command 11: Stop proxy
// ---------------------------------------------------------------------------

#[tauri::command]
fn stop_proxy(state: tauri::State<'_, ProxyState>) -> Result<String, String> {
    let mut guard = state.child.lock().map_err(|e| e.to_string())?;

    let mut diag_parts: Vec<String> = Vec::new();

    match guard.take() {
        Some(mut child) => {
            let pid = child.id();
            diag_parts.push(format!("Managed child existed, PID={}", pid));

            let kill_res = child.kill();
            let kill_msg = match &kill_res {
                Ok(()) => "kill succeeded".to_string(),
                Err(e) => format!("kill failed: {}", e),
            };
            diag_parts.push(kill_msg);

            let wait_res = child.wait();
            let wait_msg = match &wait_res {
                Ok(status) => format!("wait succeeded, exit code={:?}", status.code()),
                Err(e) => format!("wait failed: {}", e),
            };
            diag_parts.push(wait_msg);
        }
        None => {
            diag_parts.push("No managed child existed".to_string());
        }
    };

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
    let mut guard = state.child.lock().map_err(|e| e.to_string())?;

    if let Some(ref mut child) = *guard {
        match child.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                Ok(false)
            }
            Ok(None) => Ok(true),
            Err(e) => Err(format!("Cannot check child status: {}", e)),
        }
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
            start_proxy,
            stop_proxy,
            proxy_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
