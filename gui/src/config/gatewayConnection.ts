// Single source of truth for the gateway connection the app tells clients to use.
// Both the Claude config JSON panel and the Claude Code launch-command generator
// reference these, so host/port/token are never hardcoded twice.

export const GATEWAY_DEFAULT_HOST = "127.0.0.1";
export const GATEWAY_DEFAULT_PORT = 4000;
export const GATEWAY_LOCAL_TOKEN = "sk-local-gateway";

// server.host may be a bind address ("0.0.0.0", "::", "[::]"). Clients can't
// connect to those, so normalize them to the loopback address the client should use.
export function buildGatewayClientBaseUrl(host?: string, port?: number): string {
  const normalizedHost =
    !host || host === "0.0.0.0" || host === "::" || host === "[::]"
      ? GATEWAY_DEFAULT_HOST
      : host;
  return `http://${normalizedHost}:${port ?? GATEWAY_DEFAULT_PORT}`;
}
