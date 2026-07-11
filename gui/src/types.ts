// ---- Health ----
export interface HealthStatus {
  status: string;
  upstream: string;
}

// ---- Port process ----
export interface PortProcess {
  pid: string;
  raw_output: string;
}

// ---- Config ----
export interface ModelEntry {
  upstream_model: string;
  thinking_mode?: string; // "normal" | "thinking" | "thinking_only" — user choice
  // Deprecated capability fields — read for backward compat, not written on save.
  // Capabilities are resolved from MODEL_CAPABILITIES (app code) by upstream_model name.
  thinking?: string;
  supports_vision?: boolean;
  supports_video?: boolean;
  visible?: boolean;
  force_thinking?: boolean;
  supports_non_thinking?: boolean;
  reasoning_effort?: string;
  supports_image_url?: boolean;
  supports_image_base64?: boolean;
  supports_video_url?: boolean;
  supports_video_base64?: boolean;
}

export interface ProviderConfig {
  display_name: string;
  upstream_url: string;
  api_key_env: string;
  default_model: string;
  force_anthropic_version: string | null;
  supports_count_tokens: boolean;
  supports_vision: boolean;
  supports_video: boolean;
  supports_thinking: boolean;
  model_map: Record<string, string>;
  visible_models: string[];
  models?: Record<string, ModelEntry>;
}

export interface ServerConfig {
  host: string;
  port: number;
  enable_cors: boolean;
}

export interface GatewayConfig {
  config_version?: string;
  active_provider: string | null;
  providers: Record<string, ProviderConfig>;
  server: ServerConfig;
  non_vision_image_policy?: string;
}

// ---- API Key ----
export interface ApiKeyStatus {
  set: boolean;
  env_var: string;
}

export type AllApiKeyStatus = Record<string, ApiKeyStatus>;

// ---- Log ----
export interface LogContent {
  filename: string;
  content: string;
  line_count: number;
}

// ---- Log list entry ----
export interface LogListEntry {
  filename: string;
  size: number;
}

// ---- Raw config (for editing with encoding) ----
export interface RawConfigResponse {
  content: string;
  encoding_used: string;
  config_path: string;
}

// ---- Write config response ----
export interface WriteConfigResponse {
  saved_encoding: string;
}

// ---- Claude config discovery ----
export interface ClaudeConfigCandidate {
  path: string;
  exists: boolean;
  likely_config: boolean;
}

// ---- Proxy start result ----
export interface StartProxyResult {
  success: boolean;
  pid: number;
  python: string;
  dir: string;
  log: string;
}

// ---- Gateway status (used by dashboard) ----
export interface GatewayStatus {
  reachable: boolean;
  port_listening: boolean;
  checked_at: string;
  error: string | null;
  managed_child_running: boolean;
  managed_child_pid: number | null;
  diagnostic: string;
}

// ---- Hook shape ----
export interface AsyncState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refresh: () => void;
}
