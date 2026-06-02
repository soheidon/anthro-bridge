"""
Anthropic Provider Gateway — Multi-Provider Proxy

Accepts Anthropic Messages API requests and forwards them to a configurable
upstream provider (DeepSeek, MiniMax, Kimi, etc.) that also speaks the
Anthropic Messages API. Only the `model` field is rewritten; all content
blocks (text, image, video, thinking, tool_use, tool_result) and streaming
SSE events pass through byte-for-byte.

Important: This gateway MUST NOT normalize Anthropic content blocks into
plain text. It must preserve the exact content array returned by upstream
providers, including thinking, text, tool_use, tool_result, image, and
video blocks. The gateway may rewrite only the model field unless an
explicit provider-specific transform is implemented later.

Usage:
    python proxy_server.py
"""

import json
import os
import sys
import logging
from datetime import date
from typing import Optional

import httpx
from contextlib import asynccontextmanager
from fastapi import FastAPI, Request, Response
from fastapi.exceptions import HTTPException
from fastapi.responses import StreamingResponse, JSONResponse

# ---------------------------------------------------------------------------
# Early startup logging
# ---------------------------------------------------------------------------

_early_log_path = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "Communication-Logs",
    "uvicorn-stdout-stderr.log",
)
os.makedirs(os.path.dirname(_early_log_path), exist_ok=True)

_early_logger = logging.getLogger("proxy_early")
_early_logger.setLevel(logging.DEBUG)
_early_handler = logging.FileHandler(_early_log_path, encoding="utf-8", mode="a")
_early_handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
_early_logger.handlers.clear()
_early_logger.addHandler(_early_handler)
_early_logger.propagate = False

_early_logger.info("=== proxy_server.py early startup ===")
_early_logger.info("sys.executable: %s", sys.executable)
_early_logger.info("os.getcwd(): %s", os.getcwd())
_early_logger.info("__file__: %s", __file__)
_early_logger.info("=== early startup done ===")

# ---------------------------------------------------------------------------
# Config loading
# ---------------------------------------------------------------------------

CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")


def _read_config_json(path: str) -> dict:
    """Read config.json, trying UTF-8 first then Shift-JIS (for Japanese Windows)."""
    with open(path, "rb") as f:
        raw = f.read()
    for enc in ("utf-8", "utf-8-sig", "shift_jis", "cp932"):
        try:
            return json.loads(raw.decode(enc))
        except (UnicodeDecodeError, json.JSONDecodeError):
            continue
    return json.loads(raw.decode("utf-8", errors="replace"))


def _is_old_format(cfg: dict) -> bool:
    """Detect old flat config format (model_map at top level, no providers key)."""
    return "model_map" in cfg and "providers" not in cfg


def _normalize_old_config(cfg: dict) -> dict:
    """Convert old flat config to new multi-provider format."""
    return {
        "active_provider": "_legacy_deepseek",
        "providers": {
            "_legacy_deepseek": {
                "display_name": "DeepSeek (Legacy)",
                "upstream_url": cfg.get("upstream_url", "https://api.deepseek.com/anthropic"),
                "api_key_env": "DEEPSEEK_API_KEY",
                "default_model": cfg.get("default_model", "deepseek-chat"),
                "force_anthropic_version": cfg.get("force_anthropic_version"),
                "supports_count_tokens": False,
                "supports_vision": False,
                "supports_video": False,
                "supports_thinking": True,
                "model_map": cfg.get("model_map", {}),
                "visible_models": cfg.get("visible_models", []),
            }
        },
        "server": {
            "host": "127.0.0.1",
            "port": 4000,
            "enable_cors": cfg.get("enable_cors", False),
        },
    }


def _load_config(path: str) -> dict:
    """Load config, normalizing old format to new if needed."""
    raw = _read_config_json(path)
    if _is_old_format(raw):
        _early_logger.info("Detected old config format — normalizing to multi-provider")
        return _normalize_old_config(raw)
    # Ensure server section exists (fill defaults if missing)
    if "server" not in raw:
        raw["server"] = {"host": "127.0.0.1", "port": 4000, "enable_cors": False}
    raw["server"].setdefault("host", "127.0.0.1")
    raw["server"].setdefault("port", 4000)
    raw["server"].setdefault("enable_cors", False)
    return raw


def _get_active_provider(cfg: dict) -> dict:
    """Return the active provider dict from config."""
    active = cfg["active_provider"]
    if active not in cfg["providers"]:
        raise SystemExit(
            f"Active provider '{active}' not found in config.providers. "
            f"Available: {list(cfg['providers'].keys())}"
        )
    return cfg["providers"][active]


config = _load_config(CONFIG_PATH)
provider = _get_active_provider(config)

UPSTREAM_URL: str = provider["upstream_url"]
MODEL_MAP: dict[str, str] = provider["model_map"]
DEFAULT_MODEL: str = provider["default_model"]
VISIBLE_MODELS: list[str] = provider.get("visible_models", list(MODEL_MAP.keys()))
FORCE_ANTHROPIC_VERSION: Optional[str] = provider.get("force_anthropic_version")
SUPPORTS_COUNT_TOKENS: bool = provider.get("supports_count_tokens", False)
SUPPORTS_VISION: bool = provider.get("supports_vision", False)
SUPPORTS_VIDEO: bool = provider.get("supports_video", False)
SUPPORTS_THINKING: bool = provider.get("supports_thinking", True)
API_KEY_ENV: str = provider["api_key_env"]
API_KEY: str = os.environ.get(API_KEY_ENV, "")

SERVER_HOST: str = config["server"]["host"]
SERVER_PORT: int = config["server"]["port"]
ENABLE_CORS: bool = config["server"]["enable_cors"]

TIMEOUT = httpx.Timeout(
    connect=30.0,
    read=300.0,
    write=60.0,
    pool=30.0,
)

# ---------------------------------------------------------------------------
# Logging — never log the API key
# ---------------------------------------------------------------------------

LOG_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "Communication-Logs")


class RedactingFormatter(logging.Formatter):
    """Redact the active provider's API key from all log output."""

    def __init__(self, fmt: str, datefmt: str, keys_to_redact: list[str]):
        super().__init__(fmt, datefmt)
        self.keys_to_redact = [k for k in keys_to_redact if k and len(k) > 4]

    def format(self, record: logging.LogRecord) -> str:
        msg = super().format(record)
        for key in self.keys_to_redact:
            msg = msg.replace(key, "<REDACTED>")
        return msg


# Collect all API keys from all providers for redaction
_all_keys = []
for _p in config.get("providers", {}).values():
    _k = os.environ.get(_p.get("api_key_env", ""), "")
    if _k:
        _all_keys.append(_k)

formatter = RedactingFormatter(
    "%(asctime)s [%(levelname)s] %(message)s", "%H:%M:%S", _all_keys
)

console_handler = logging.StreamHandler(sys.stdout)
console_handler.setFormatter(formatter)

os.makedirs(LOG_DIR, exist_ok=True)
file_handler = logging.FileHandler(
    os.path.join(LOG_DIR, f"proxy-{date.today().isoformat()}.log"),
    encoding="utf-8",
)
file_handler.setFormatter(formatter)

logger = logging.getLogger("proxy")
logger.setLevel(logging.INFO)
logger.handlers.clear()
logger.addHandler(console_handler)
logger.addHandler(file_handler)
logger.propagate = False

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def rewrite_model(requested_model: str) -> str:
    """Rewrite model name. Falls back to provider default if not in map."""
    mapped = MODEL_MAP.get(requested_model)
    if mapped is None:
        logger.warning(
            f"Unknown incoming model: {requested_model} "
            f"-> Fallback to default_model: {DEFAULT_MODEL}"
        )
        return DEFAULT_MODEL
    return mapped


def upstream_headers(incoming_headers: dict) -> dict:
    """Build headers for the upstream provider request."""
    headers = {}

    headers["Authorization"] = f"Bearer {API_KEY}"
    headers["Content-Type"] = "application/json"

    if FORCE_ANTHROPIC_VERSION:
        headers["anthropic-version"] = FORCE_ANTHROPIC_VERSION
    elif "anthropic-version" in incoming_headers:
        headers["anthropic-version"] = incoming_headers["anthropic-version"]

    if "anthropic-beta" in incoming_headers:
        headers["anthropic-beta"] = incoming_headers["anthropic-beta"]

    return headers


def safe_log_request(method: str, path: str, body: dict):
    """Log request metadata without sensitive content."""
    model_in = body.get("model", "?")
    model_out = rewrite_model(model_in)
    stream = body.get("stream", False)
    tools = bool(body.get("tools"))
    msg_count = len(body.get("messages", []))
    logger.info(
        f"{method} {path} | model: {model_in} -> {model_out}"
        f" | stream={stream} | tools={tools} | msgs={msg_count}"
    )


def _detect_media_types(messages: list[dict]) -> set[str]:
    """Scan messages for image/video content blocks. Returns set of media types found."""
    found: set[str] = set()
    for msg in messages:
        content = msg.get("content", [])
        if isinstance(content, str):
            continue
        if not isinstance(content, list):
            continue
        for block in content:
            if not isinstance(block, dict):
                continue
            t = block.get("type", "")
            if t in ("image", "video"):
                found.add(t)
    return found


class UnsupportedMediaError(HTTPException):
    """Raised when the request contains image/video blocks but the provider doesn't support them."""

    def __init__(self, detail: str):
        super().__init__(status_code=400, detail=detail)


def _check_media_support(messages: list[dict]):
    """Raise an HTTPException if messages contain unsupported media blocks."""
    media_types = _detect_media_types(messages)
    provider_name = provider.get("display_name", config["active_provider"])
    if "image" in media_types and not SUPPORTS_VISION:
        raise UnsupportedMediaError(
            f"Provider '{provider_name}' does not support image input. "
            f"Switch active_provider to a vision-capable provider (e.g. minimax or kimi)."
        )
    if "video" in media_types and not SUPPORTS_VIDEO:
        raise UnsupportedMediaError(
            f"Provider '{provider_name}' does not support video input. "
            f"Switch active_provider to a provider with video support."
        )


def log_startup_info():
    """Log provider-agnostic startup information."""
    logger.info(f"Active provider: {config['active_provider']}")
    logger.info(f"Display name: {provider.get('display_name', config['active_provider'])}")
    logger.info(f"Upstream URL: {UPSTREAM_URL}")
    logger.info(f"API key env: {API_KEY_ENV}")
    logger.info(f"API key present: {bool(API_KEY)}")
    logger.info(f"API key length: {len(API_KEY)}")
    logger.info(f"Default model: {DEFAULT_MODEL}")
    logger.info(f"Model map: {MODEL_MAP}")
    logger.info(f"Count tokens supported: {SUPPORTS_COUNT_TOKENS}")
    logger.info(f"Listening on: {SERVER_HOST}:{SERVER_PORT}")

# ---------------------------------------------------------------------------
# Lifespan
# ---------------------------------------------------------------------------


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup
    if not API_KEY:
        logger.error(
            f"API key not set: environment variable '{API_KEY_ENV}' is missing."
        )
        sys.exit(1)
    log_startup_info()
    yield
    # Shutdown
    await _shutdown_client()


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------

app = FastAPI(title="Anthropic Provider Gateway", version="0.2.0", lifespan=lifespan)

if ENABLE_CORS:
    from fastapi.middleware.cors import CORSMiddleware

    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_methods=["*"],
        allow_headers=["*"],
    )


@app.get("/health")
async def health():
    return {
        "status": "ok",
        "upstream": UPSTREAM_URL,
        "provider": config["active_provider"],
        "api_key_env": API_KEY_ENV,
        "api_key_set": bool(API_KEY),
    }


@app.get("/v1/models")
async def list_models():
    """Return visible model names from the active provider's config."""
    return {
        "object": "list",
        "data": [
            {"id": m, "object": "model", "type": "model"} for m in VISIBLE_MODELS
        ],
    }


@app.api_route("/v1/messages", methods=["POST"])
async def proxy_messages(request: Request):
    body = await request.json()
    safe_log_request("POST", "/v1/messages", body)

    # Check media support before forwarding
    _check_media_support(body.get("messages", []))

    # Rewrite model name only — all content blocks pass through untouched
    body["model"] = rewrite_model(body.get("model", DEFAULT_MODEL))
    is_stream = body.get("stream", False)

    upstream_req = httpx.Request(
        "POST",
        f"{UPSTREAM_URL}/v1/messages",
        json=body,
        headers=upstream_headers(dict(request.headers)),
    )

    client = _get_client()
    if is_stream:
        return await _handle_stream(client, upstream_req)
    else:
        return await _handle_nonstream(client, upstream_req)


async def _handle_nonstream(client: httpx.AsyncClient, req: httpx.Request):
    try:
        resp = await client.send(req)
    except httpx.RequestError as exc:
        logger.error(f"Upstream request failed: {exc}")
        return JSONResponse(
            {"error": {"type": "proxy_error", "message": str(exc)}},
            status_code=502,
        )
    return Response(
        content=resp.content,
        status_code=resp.status_code,
        headers=dict(resp.headers),
    )


async def _handle_stream(client: httpx.AsyncClient, req: httpx.Request):
    async def stream_sse():
        try:
            async with client.stream(
                req.method, req.url, headers=req.headers, content=req.content
            ) as resp:
                if resp.status_code != 200:
                    body = await resp.aread()
                    logger.error(
                        f"Stream upstream error {resp.status_code}: {body[:300]}"
                    )
                    yield f'data: {{"error": "upstream error {resp.status_code}"}}\n\n'
                    return

                async for chunk in resp.aiter_bytes(chunk_size=4096):
                    yield chunk
        except httpx.RequestError as exc:
            logger.error(f"Stream upstream failed: {exc}")
            yield f'data: {{"error": "upstream connection failed: {exc}"}}\n\n'

    return StreamingResponse(
        stream_sse(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


@app.api_route("/v1/messages/count_tokens", methods=["POST"])
async def proxy_count_tokens(request: Request):
    if not SUPPORTS_COUNT_TOKENS:
        return JSONResponse(
            {
                "type": "error",
                "error": {
                    "type": "not_supported_error",
                    "message": (
                        f"The active provider '{config['active_provider']}' "
                        f"does not support /v1/messages/count_tokens."
                    ),
                },
            },
            status_code=501,
        )

    body = await request.json()
    safe_log_request("POST", "/v1/messages/count_tokens", body)

    body["model"] = rewrite_model(body.get("model", DEFAULT_MODEL))

    client = _get_client()
    try:
        resp = await client.post(
            f"{UPSTREAM_URL}/v1/messages/count_tokens",
            json=body,
            headers=upstream_headers(dict(request.headers)),
        )
    except httpx.RequestError as exc:
        logger.error(f"count_tokens request failed: {exc}")
        return JSONResponse(
            {"error": {"type": "proxy_error", "message": str(exc)}},
            status_code=502,
        )

    return Response(
        content=resp.content,
        status_code=resp.status_code,
        headers=dict(resp.headers),
    )


# ---------------------------------------------------------------------------
# HTTP client
# ---------------------------------------------------------------------------

_client: Optional[httpx.AsyncClient] = None


def _get_client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(timeout=TIMEOUT)
    return _client


async def _shutdown_client():
    global _client
    if _client is not None:
        await _client.aclose()
        _client = None


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import uvicorn

    logger.info(f"Starting uvicorn on {SERVER_HOST}:{SERVER_PORT}")
    try:
        uvicorn.run(
            app,
            host=SERVER_HOST,
            port=SERVER_PORT,
            log_level="info",
            access_log=False,
            log_config=None,
        )
    except Exception:
        logger.exception("uvicorn.run() raised an exception")
        _early_logger.exception("uvicorn.run() raised an exception")
        raise
