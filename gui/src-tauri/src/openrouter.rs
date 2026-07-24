use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

// ---------------------------------------------------------------------------
// API response types (raw, matches OpenRouter API structure)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OpenRouterApiResponse {
    pub data: Vec<OpenRouterApiModel>,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterApiModel {
    pub id: String,
    pub canonical_slug: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub architecture: Option<OpenRouterArchitecture>,
    pub supported_parameters: Option<Vec<String>>,
    pub pricing: Option<OpenRouterApiPricing>,
    pub top_provider: Option<OpenRouterTopProvider>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OpenRouterArchitecture {
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterTopProvider {
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OpenRouterApiPricing {
    pub prompt: Option<String>,
    pub completion: Option<String>,
    pub image: Option<String>,
    pub request: Option<String>,
    pub audio: Option<String>,
    pub web_search: Option<String>,
    pub internal_reasoning: Option<String>,
    pub input_cache_read: Option<String>,
    pub input_cache_write: Option<String>,
}

// ---------------------------------------------------------------------------
// Normalized cache types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModelCache {
    pub schema_version: u32,
    pub fetched_at: String,
    pub models: Vec<OpenRouterModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModel {
    pub id: String,
    pub canonical_slug: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub supported_parameters: Vec<String>,
    pub pricing: OpenRouterPricing,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterPricing {
    pub prompt: Option<String>,
    pub completion: Option<String>,
    pub image: Option<String>,
    pub request: Option<String>,
    pub audio: Option<String>,
    pub web_search: Option<String>,
    pub internal_reasoning: Option<String>,
    pub input_cache_read: Option<String>,
    pub input_cache_write: Option<String>,
}

impl From<OpenRouterApiPricing> for OpenRouterPricing {
    fn from(raw: OpenRouterApiPricing) -> Self {
        Self {
            prompt: raw.prompt,
            completion: raw.completion,
            image: raw.image,
            request: raw.request,
            audio: raw.audio,
            web_search: raw.web_search,
            internal_reasoning: raw.internal_reasoning,
            input_cache_read: raw.input_cache_read,
            input_cache_write: raw.input_cache_write,
        }
    }
}

impl From<OpenRouterApiModel> for OpenRouterModel {
    fn from(raw: OpenRouterApiModel) -> Self {
        let arch = raw.architecture.unwrap_or_default();
        let top_ctx = raw
            .top_provider
            .as_ref()
            .and_then(|t| t.context_length);
        let max_completion = raw
            .top_provider
            .and_then(|t| t.max_completion_tokens);

        Self {
            id: raw.id,
            canonical_slug: raw.canonical_slug,
            display_name: raw.name,
            description: raw.description,
            context_length: raw.context_length.or(top_ctx),
            max_completion_tokens: max_completion,
            input_modalities: arch.input_modalities,
            output_modalities: arch.output_modalities,
            supported_parameters: raw.supported_parameters.unwrap_or_default(),
            pricing: raw.pricing.unwrap_or_default().into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Capability resolution (used by proxy.rs)
// ---------------------------------------------------------------------------

/// Resolve capabilities for an OpenRouter model from the cache.
/// Returns `None` if the model is not in the cache (custom/unknown model).
pub fn resolve_capabilities_from_cache(
    model_id: &str,
    cached_models: &[OpenRouterModel],
) -> Option<(bool, bool, bool, bool)> {
    // Returns (supports_vision, supports_video, supports_thinking, supports_tools)
    let model = cached_models.iter().find(|m| m.id == model_id)?;
    let supports_vision = model.input_modalities.iter().any(|m| m == "image");
    let supports_video = model.input_modalities.iter().any(|m| m == "video");
    // OpenRouter standard is `reasoning`. `thinking` is legacy/compat fallback.
    let supports_thinking = model.supported_parameters.iter().any(|p| p == "reasoning")
        || model.supported_parameters.iter().any(|p| p == "thinking");
    let supports_tools = model.supported_parameters.iter().any(|p| p == "tools")
        || model
            .supported_parameters
            .iter()
            .any(|p| p == "tool_choice");
    Some((
        supports_vision,
        supports_video,
        supports_thinking,
        supports_tools,
    ))
}

// ---------------------------------------------------------------------------
// Tauri command: openrouter_get_models
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterModelsResult {
    pub models: Vec<OpenRouterModel>,
    pub fetched_at: String,
    pub source: String, // "network" | "cache"
    pub stale: bool,
    pub warning: Option<String>,
}

fn cache_path(app_data_dir: &std::path::Path) -> PathBuf {
    app_data_dir.join("openrouter_models.json")
}

fn load_cache(app_data_dir: &std::path::Path) -> Option<OpenRouterModelCache> {
    let path = cache_path(app_data_dir);
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_cache(app_data_dir: &std::path::Path, cache: &OpenRouterModelCache) -> Result<(), String> {
    let path = cache_path(app_data_dir);
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(&tmp_path, &json).map_err(|e| format!("Failed to write cache: {}", e))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename cache file: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn openrouter_get_models(
    app: tauri::AppHandle,
    force_refresh: bool,
) -> Result<OpenRouterModelsResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    if !force_refresh {
        if let Some(cache) = load_cache(&app_data_dir) {
            if !cache.models.is_empty() {
                return Ok(OpenRouterModelsResult {
                    models: cache.models,
                    fetched_at: cache.fetched_at,
                    source: "cache".to_string(),
                    stale: false,
                    warning: None,
                });
            }
        }
    }

    // Fetch from network
    match fetch_models_from_api(&app_data_dir) {
        Ok(result) => Ok(result),
        Err(e) => {
            // Try falling back to stale cache
            if let Some(cache) = load_cache(&app_data_dir) {
                Ok(OpenRouterModelsResult {
                    models: cache.models,
                    fetched_at: cache.fetched_at,
                    source: "cache".to_string(),
                    stale: true,
                    warning: Some(e),
                })
            } else {
                Err(e)
            }
        }
    }
}

fn fetch_models_from_api(
    app_data_dir: &std::path::Path,
) -> Result<OpenRouterModelsResult, String> {
    // Resolve API key
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| "OPENROUTER_API_KEY not set — set it in the API Key tab first.".to_string())?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("HTTP-Referer", "https://github.com/soheidon/anthro-bridge")
        .header("X-OpenRouter-Title", "Anthro Bridge")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                "Request timed out".to_string()
            } else if e.is_connect() {
                "Network connection failed".to_string()
            } else {
                format!("Request failed: {}", e)
            }
        })?;

    let status = resp.status();

    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        let warning = match status.as_u16() {
            401 => "API key is invalid".to_string(),
            402 => "Insufficient credits".to_string(),
            403 => format!("Access denied: {}", &body[..body.len().min(200)]),
            429 => "Rate limited — try again later".to_string(),
            500..=599 => format!("OpenRouter server error (HTTP {})", status.as_u16()),
            _ => format!("HTTP {}: {}", status.as_u16(), &body[..body.len().min(200)]),
        };
        return Err(warning);
    }

    let api_resp: OpenRouterApiResponse = resp
        .json()
        .map_err(|e| format!("Failed to parse model list: {}", e))?;

    let models: Vec<OpenRouterModel> = api_resp
        .data
        .into_iter()
        .map(OpenRouterModel::from)
        .collect();

    let fetched_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string();

    let cache = OpenRouterModelCache {
        schema_version: 1,
        fetched_at: fetched_at.clone(),
        models: models.clone(),
    };

    // Atomic save
    if let Err(e) = save_cache(app_data_dir, &cache) {
        tracing::warn!("Failed to save OpenRouter model cache: {}", e);
        return Ok(OpenRouterModelsResult {
            models,
            fetched_at,
            source: "network".to_string(),
            stale: false,
            warning: Some(e),
        });
    }

    Ok(OpenRouterModelsResult {
        models,
        fetched_at,
        source: "network".to_string(),
        stale: false,
        warning: None,
    })
}

/// Load cached models for use by proxy.rs capability resolution.
/// Returns empty vec if no cache exists.
pub fn load_cached_models(app_data_dir: &std::path::Path) -> Vec<OpenRouterModel> {
    load_cache(app_data_dir)
        .map(|c| c.models)
        .unwrap_or_default()
}
