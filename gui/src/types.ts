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
}

export interface ServerConfig {
  host: string;
  port: number;
  enable_cors: boolean;
}

export interface GatewayConfig {
  active_provider: string;
  providers: Record<string, ProviderConfig>;
  server: ServerConfig;
}

// ---- API Key ----
export interface ApiKeyStatus {
  set: boolean;
  env_var: string;
}

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
