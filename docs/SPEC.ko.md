[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## 개요

여러 제공자의 Anthropic 호환 엔드포인트를 통해 Claude Desktop / Claude Code API 요청을 라우팅하는 가벼운 프록시 + GUI 관리 도구입니다.

### 아키텍처

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Tauri 앱에 내장 (axum 0.7 + reqwest)
       |
       | 모델 필드별 라우팅 -> 올바른 업스트림 제공자 확인
       | 모델을 업스트림 이름으로만 다시 쓰기
       | 비-thinking 변형에 thinking disabled 주입
       | 모델별 미디어 지원 확인
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### 설계 원칙

- **쉘 모델 + 제공자 선택**: Claude Desktop에는 항상 `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`가 표시됩니다. 실제 LLM은 GUI에서 선택합니다 (DeepSeek / MiniMax / Kimi / MiMo / OpenRouter). 활성 제공자의 모델 매핑이 라우팅에 사용됩니다.
- **OpenRouter 지원**: Poolside Laguna S/XS 기본값을 사용하여 OpenRouter의 Anthropic 호환 엔드포인트로 라우팅합니다. 전용 thinking 모드 컨트롤(Max/On/Off)은 요청 시 OpenRouter의 `reasoning` 형식으로 변환됩니다.
- **활성 제공자만 API 키 필요**: v0.5.0부터 시작 시 라우팅 테이블에서 참조하는 제공자만 확인합니다. 비활성 제공자의 키는 필요하지 않습니다.
- **가벼운 프록시**: `model` 필드 외에는 아무것도 수정하지 않습니다. SSE는 바이트 단위로 그대로 전달됩니다.
- **무손실 전달**: 메시지 본문, 도구 호출, thinking 블록이 수정 없이 전달됩니다.
- **Windows 네이티브 GUI**: Tauri v2 + React 19 + TypeScript. Rust 백엔드, Vite + React 19 프론트엔드.
- **외부 의존성 제로**: v0.3.0부터 프록시가 Tauri 바이너리에 내장됩니다. Python이 필요하지 않습니다.
- **다국어**: 8개 언어 지원 (en, ja, zh-CN, zh-TW, ko, fr, de, es). `lang/`에 파일을 넣으면 새 언어가 추가됩니다. 첫 실행 시 언어 선택기.
- **추론 강도**: DeepSeek V4 Pro(V4-Pro-0813)와 V4 Flash(V4-Flash-0731) 모두 Thinking 모드에서 추론 강도 Low / High / Max를 지원합니다. 추론 강도는 일반 모드에서 비활성화됩니다. V4 Pro 라우트에 저장된 레거시 `medium`/`xhigh` 강도는 시작 시 `high`로 마이그레이션됩니다. 프록시는 DeepSeek에 전송하기 전에 강도 값을 정규화(`medium`/`xhigh` → `high`)하고 `output_config.effort` 형식으로 전송합니다.
- **기능 감지**: OpenRouter API에서 가져온 실시간 기능 플래그 (supports_image_url, supports_image_base64, supports_video_url, supports_video_base64)를 config.json에 저장합니다.
- **피크/밸리 가격 인식**: DeepSeek 및 OpenRouter의 피크 시간대를 현지 시간대로 표시합니다.
- **MiniMax-M3 thinking 토글**: MiniMax-M3는 Anthropic 호환 API를 통해 Thinking ON/OFF를 지원합니다 (`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`). M2.x 모델은 Thinking 전용으로 유지됩니다. 시작 시 마이그레이션은 기존 사용자의 레거시 `thinking_only` → `thinking`을 변환합니다.
- **응답 모델 ID 정규화**: API 응답(SSE 스트리밍 및 비스트리밍 모두)의 업스트림 모델 이름을 Anthropic 공식 모델 이름으로 다시 씁니다. config.json의 `normalize_response_model_identity`와 런타임 `AtomicBool`로 제어됩니다. 서버 설정 저장과의 상호 오염을 피하기 위해 독립적인 저장 명령(`update_normalize_model_identity`)을 사용합니다.
- **구조화된 통신 로깅**: `tracing` + `tracing-appender`가 `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`에 구조화된 로그를 기록합니다. 각 요청은 `AtomicU64` 카운터에서 상관관계 ID를 받습니다. 로그 항목에는 요청 모델, 게이트웨이 모델, 업스트림 모델, 정규화 결과 및 건너뛴 이유가 포함됩니다. 민감한 데이터(프롬프트, 본문, API 키)는 기록되지 않습니다.
- **PEAK 배지**: 피크 가격 모델에 대해 대시보드에 색상 구분된 분홍색 배지를 표시합니다.
- **UTC 오프셋 표시**: 시간대 선택기에서 각 옵션 옆에 동적 UTC 오프셋(예: UTC+09:00)을 표시합니다.
- **Laguna S/XS 2.1 토큰 상한 실패 감지**: SSE 스트림과 비스트리밍 응답 모두에서 `stop_reason: "max_tokens"`가 있는 추론 전용 응답을 감지합니다. 사용 가능한 텍스트나 도구 호출을 생성하지 않고 턴당 토큰 상한에 도달하면 경고를 기록합니다. OpenRouter를 통해 모든 Poolside Laguna 모델에 사용할 수 있습니다.
- **Poolside thinking:disabled 전달**: 클라이언트가 보낸 `thinking: { type: "disabled" }`를 Poolside 모델의 OpenRouter `reasoning: { enabled: false }` 형식으로 변환하여, 저장된 설정이 없어도 disabled thinking이 올바르게 전달되도록 합니다.
- **Laguna Opus 기본값 마이그레이션**: 일회성 멱등 마이그레이션으로 `poolside/laguna-s-2.1` OpenRouter 사용자의 `claude-opus-5` 기본값을 thinking 켜짐에서 일반 모드로 변경합니다. 새 설치 템플릿은 업데이트된 기본값을 반영합니다.
- **OpenRouter 다중 프로필**: 사용자당 여러 OpenRouter 프로필을 지원하며, 각 프로필은 자체 API 키와 모델 구성을 가집니다. 프로필 CRUD는 Tauri 명령을 통해 수행됩니다. 대시보드 또는 설정에서 활성 프로필을 전환합니다. 프로필은 드래그 앤 드롭으로 재정렬할 수 있고 숨길 수 있으며 설정된 순서대로 저장됩니다.
- **OpenRouter 대시보드 카드**: 대시보드는 표시되는 각 OpenRouter 프로필당 카드 하나를 만들며, 프로필이 없으면 대체 카드를 표시합니다. 모델 요약은 OpenRouter 표시용으로만 첫 번째 `/` 이전의 벤더 네임스페이스를 숨깁니다. 라우팅을 위한 전체 업스트림 ID는 그대로 유지됩니다.
- **OpenRouter 모델 레지스트리**: 사전 구성된 기능(비전, 비디오, thinking 정책, 추론 강도), 벤더 그룹화 및 가격 데이터를 포함하는 알려진 OpenRouter 모델의 로컬 내장 레지스트리(`model_capabilities.rs`, `builtinOpenRouter.ts`). 실시간 API 호출 없이 모델 분류에 사용됩니다.
- **OpenRouter 가격 세부 정보**: 내장 가격 데이터는 GPT-5.6 Sol, Terra, Luna 및 Pro 변형을 포함한 프롬프트, 출력 및 캐시된 입력 요금의 현재 값과 수정된 표준 값을 지원합니다. GUI는 프로모션 및 표준 요금이 모두 제공되면 함께 표시합니다.
- **GPT-5.6 모델 지원**: OpenRouter 프로필은 Sol, Terra 및 Luna 모델 변형을 사용할 수 있으며, 기능 인식 thinking 컨트롤과 해당되는 경우 장문 컨텍스트 요금에 대한 가격 메모를 지원합니다. 내장된 OpenAI GPT-5.6 Balanced 프로필은 새 설치 시 Opus 5 → GPT-5.6 Sol, Sonnet 5 → GPT-5.6 Terra, Haiku 4.5 → GPT-5.6 Luna로 라우팅하며 세 라우트 모두에서 Thinking High 추론 강도를 사용합니다. 기존 저장된 라우팅은 자동으로 변경되지 않습니다.
- **대시보드 기반 창 크기 조정**: 초기 및 행 수 변경 시 3열 그리드의 표시되는 대시보드 카드에서 창 높이를 계산합니다. 계산은 카드 높이, 그리드 간격, 네이티브 최소 크기, 모니터 작업 영역, DPI 배율 및 창 장식 요소를 고려하며, 행 수가 변경되지 않은 경우 수동 크기 조정을 유지합니다.
- **지역화된 NSIS 설치 프로그램**: Windows 설치 프로그램은 영어, 일본어, 중국어(간체), 중국어(번체), 한국어, 프랑스어, 독일어 및 스페인어 언어 선택을 제공하며 Anthro Bridge 애플리케이션 아이콘을 포함합니다.
- **회귀 테스트 커버리지**: Vitest 커버리지에는 OpenRouter 프로필 순서 및 저장 경쟁 조건, 프로덕션 가격 데이터, 대시보드 카드 수 의미론 및 모니터 인식 창 크기 조정이 포함됩니다.
- **OpenRouter를 통한 새 제공자**: InclusionAI 및 StepFun이 전용 기능 플래그, thinking 모드 컨트롤 및 벤더 그룹화와 함께 OpenRouter 모델 제공자로 추가되었습니다.
- **Tencent Hy3 thinking 모드**: Tencent의 Hunyuan 모델에 대한 Low/High 추론 강도 지원. proxy.rs의 thinking 모드 변환은 `thinking_mode`를 OpenRouter의 `reasoning` 형식으로 매핑합니다. UI는 Low/High를 드롭다운 옵션으로 표시합니다.
- **Kimi K3 수정**: 기능 정의에서 하드코딩된 `forced_reasoning_effort`를 제거했습니다. 고정 "Max" 표시를 구성 가능한 드롭다운 선택기로 대체했습니다. 기본값은 저장된 설정에서 가져오며 "max"로 대체됩니다.
- **설정 쓰기 직렬화**: 모든 설정 쓰기 Tauri 명령은 `Mutex` 가드와 함께 `execute_serialized_config_mutation`을 통해 직렬화됩니다. `ConfigState` 구조체는 검증과 함께 `applied_config`, `in_flight_config` 및 `pending_ops` 추적을 제공합니다. 여러 설정 변경이 동시에 저장될 때 경쟁 조건을 방지합니다.
- **OpenRouter UI 경쟁 조건 수정**: (1) `syncUiFromSavedRouteRef` 최신 콜백 ref가 오래된 클로저가 새 라우트의 UI를 덮어쓰는 것을 방지합니다. (2) `rollbackRouteId` 가드가 라우트 간 Phase 2 롤백을 방지합니다. (3) `useRouteSaveGeneration` 훅이 모든 핸들러에 `begin()`/`isCurrent()` 세대 가드를 제공합니다. (4) 저장 큐 훅(`useOpenRouterSaveQueue`)은 드레인 루프, 대체 감지 및 재시작 OR-집계를 제공합니다.
- **개발/안정 앱 ID 격리**: `paths.rs`의 `AppChannel` 열거형(`Stable`/`Dev`)이 별도의 식별자(`com.soheidon.anthro-bridge` vs `.dev`), 설정 디렉터리(`Anthro Bridge` vs `Anthro Bridge Dev`) 및 캐시 경로를 선택합니다. 개발 채널은 `tauri.dev.conf.json`을 사용합니다. NPM 스크립트: `npm run dev` (dev), `npm run dev:stable` (stable).
- **설정 템플릿 내장**: `include_str!()`이 컴파일 시점에 `config_template.rs`를 내장하여 번들된 `config.json`에 대한 런타임 의존성을 제거합니다. `merge_bundled_providers`는 타입화된 오류 처리가 있는 `Result`를 반환합니다.
- **프론트엔드 회귀 테스트**: `QueueHarness` 및 `GenerationHandlerHarness`를 사용한 OpenRouter 저장 경쟁 조건에 대한 vitest 회귀 테스트 7개. 테스트 범위: 최신 콜백 ref, 라우트 간 롤백 가드, ID 캡처, 새로고침 재시도(실패 + 성공 경로), 진행 중 대체 및 세대 가드.
- **Claude Code 컨텍스트 관리**: Claude Code용 모델 인식 자동 압축. `resolve_effective_auto_compact`는 각 표준 라우트(claude-opus-5, claude-sonnet-5, claude-haiku-4-5)를 업스트림 모델로 확인하고, 정적 `model_context_windows.json` 레지스트리에서 각 모델의 컨텍스트 용량을 조회하며, Auto 모드에서는 알려진 가장 작은 용량을 안전한 컨텍스트 창으로 사용합니다. 컨텍스트 제어는 세 용량이 모두 알려진 경우에만 적용됩니다(그렇지 않으면 상태는 Incomplete). 헤더 토글로 컨텍스트 관리를 켜거나 끕니다. 고급 모드와 임계값은 `config.json`의 `claude_code.auto_compact` 아래에 설정됩니다. 모드: `auto`, `manual` (`window_tokens`), `claude_default`.
- **Claude Code 실행 명령 생성**: `build_claude_code_launch_command`는 게이트웨이 연결 변수(`ANTHROPIC_BASE_URL`은 로컬 게이트웨이를 가리킴, `ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`)와 Claude Code 컨텍스트 제어 변수(`CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`)를 결합한 완전한 PowerShell 명령을 생성합니다. 컨텍스트 관리가 비활성화되거나, 불완전하거나, Claude 기본값으로 설정된 경우 명령은 `Remove-Item Env:... -ErrorAction SilentlyContinue`로 오래된 컨텍스트 변수를 제거하여 이전에 설정된 세션 값이 새 실행에 누출되지 않도록 합니다. Claude 설정 패널의 "Claude Code 실행 명령 복사" 버튼은 명령을 클립보드에 복사합니다. Anthro Bridge는 명령을 생성하고 복사만 합니다 — 실행하지는 않습니다.
- **공유 모델 라우팅 모듈**: `model_routing.rs`는 라우트-업스트림 확인을 `proxy.rs`와 컨텍스트 리졸버가 공유하는 순수 함수로 추출하여, 컨텍스트 창이 프록시가 실제로 전달하는 것과 동일한 업스트림 모델로 확인되도록 보장합니다.
- **컨텍스트 용량 레지스트리**: `model_context_windows.json`은 내장된 직접 제공자 모델(DeepSeek, MiniMax, Kimi, MiMo)과 내장된 OpenRouter 모델(Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6)을 포함하는 알려진 컨텍스트 용량의 정적 레지스트리입니다. 알 수 없는 사용자 지정 OpenRouter 모델은 유효한 라우트 대상으로 유지되지만, 메타데이터가 추가되거나 수동 모드가 구성될 때까지 컨텍스트 관리를 Incomplete로 보고합니다.

### GUI 관리 도구

Tauri v2 + React 19 + TypeScript. 이중 패널 레이아웃: 대시보드 + 설정.

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [게이트웨이 시작/중지] [상태]    [=]     |
+------------------------------------------+
|  대시보드                                 |
|  +- LLM 제공자 선택 ------------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]   ||
|  +- 상태 --------------------------------+
|  | 포트 4000 | API 키 | 게이트웨이 URL   ||
|  | 모델 라우팅 테이블                    ||
|  +- 최신 로그 ---------------------------+
|  | Pro/Flash 카운터가 있는 로그 뷰어     ||
|  +---------------------------------------+
+------------------------------------------+

설정 (=):
  +- 언어 ------------------------------+
  | 드롭다운으로 즉시 전환               |
  +- API 키 -----------------------------+
  | 제공자별 API 키 관리                  |
  +- Claude Desktop 설정 ----------------+
  | 설정 JSON 생성, 복사,                 |
  | 설정 파일 감지                        |
  +- 게이트웨이 설정 -------------------+
  | config.json 편집기 (고급)             |
  +---------------------------------------+
```

### Tauri 명령

| # | 명령 | 타입 | 설명 |
|---|------|------|------|
| 1 | `check_health` | async | 프록시 헬스 체크 |
| 2 | `check_gateway_status` | sync | 포트 4000 + tokio 태스크 활성 상태 |
| 3 | `check_api_key` | sync | 활성 제공자 API 키 상태 |
| 4 | `set_env_api_key` | sync | setx로 API 키 영구 저장 |
| 5 | `get_port_4000_process` | sync | netstat으로 포트 4000의 PID 가져오기 |
| 6 | `read_config` | sync | config.json 읽기 |
| 7 | `read_config_raw` | sync | 원시 config.json 텍스트 + 인코딩 감지 |
| 8 | `write_config` | sync | config.json 저장 (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | 최신 로그 읽기 |
| 10 | `read_log` | sync | 지정된 로그 파일 읽기 |
| 11 | `list_logs` | sync | 로그 파일 목록 |
| 12 | `create_new_log` | sync | 새 로그 파일 생성 |
| 13 | `open_logs_folder` | sync | 로그 폴더 열기 |
| 14 | `open_path` | sync | 임의 경로 열기 |
| 15 | `find_claude_configs` | sync | Claude Desktop 설정 파일 자동 감지 |
| 16 | `start_proxy` | sync | 프록시 시작 (설정 확인 -> 실행 -> 포트 확인) |
| 17 | `stop_proxy` | sync | 프록시 중지 (우아한 종료) |
| 18 | `proxy_status` | sync | 태스크 활성 상태 확인 |
| 19 | `check_all_api_keys` | sync | 모든 제공자 API 키 상태 |
| 20 | `update_active_provider` | sync | active_provider 저장 |
| 21 | `update_provider_api_key_env` | sync | provider api_key_env 저장 |
| 22 | `get_user_language` | sync | 저장된 언어 환경설정 가져오기 |
| 23 | `set_user_language` | sync | 언어 환경설정 저장 |
| 24 | `is_first_run` | sync | 첫 실행 확인 (user_prefs.json 존재 여부) |
| 25 | `openrouter_get_models` | async | OpenRouter 모델 카탈로그 가져오기/캐시 |
| 26 | `set_model_upstream` | sync | 게이트웨이 모델의 업스트림 모델 + thinking 설정 + 기능 플래그 저장 |
| 27 | `update_server_config` | sync | 서버 호스트/포트/CORS 설정 저장 |
| 28 | `update_normalize_model_identity` | sync | 응답 모델 ID 정규화 토글 저장 (config + 런타임 AtomicBool 업데이트) |
| 29 | `update_claude_code_auto_compact_global` | sync | 전역 Claude Code 컨텍스트 관리 전환 (enabled + trigger percent) |
| 30 | `update_claude_code_auto_compact_target` | sync | 제공자/프로필별 컨텍스트 모드 설정 (auto / manual / claude_default) + 수동 window tokens |
| 31 | `update_claude_code_context_settings` | sync | 전역 + 대상 컨텍스트 설정의 결합된 원자적 업데이트 |
| 32 | `resolve_claude_code_auto_compact` | sync | 유효 컨텍스트 설정 확인 (mode, window tokens, trigger percent, status) |
| 33 | `build_claude_code_launch_command` | sync | 전체 PowerShell Claude Code 실행 명령 생성 (게이트웨이 + 컨텍스트 환경 변수) |

### 프록시 서버 (proxy.rs)

v0.3.0에서 Python에서 Rust (axum 0.7/reqwest)로 포팅됨.

#### 엔드포인트

| 메서드 | 경로 | 동작 |
|--------|------|------|
| GET | `/health` | 헬스 체크 |
| GET | `/v1/models` | 공개 모델 목록 (`visible: true`만) |
| POST | `/v1/messages` | 모델 확인 -> thinking 주입 -> 미디어 확인 -> 전달 (stream/non-stream) |
| POST | `/v1/messages/count_tokens` | 지원되는 경우 업스트림으로 전달 |

#### 모델 라우팅

각 제공자의 `models` 섹션을 사용하여 게이트웨이 모델 -> (제공자, 업스트림 모델)의 역방향 조회 테이블을 구축합니다. 모든 제공자가 동일한 게이트웨이 모델 이름을 사용하므로 충돌 시 `active_provider`가 우선합니다. 실제로 라우팅 테이블에는 활성 제공자의 모델만 들어갑니다.

#### API 키 검증 (v0.5.0부터)

1단계: 모델 라우팅 테이블 구축 (API 키 불필요)
2단계: 라우팅 테이블에서 참조하는 제공자의 API 키만 확인

#### Thinking 주입

구성 항목에 `thinking: "disabled"`가 있는 모델의 경우, 사용자가 thinking을 명시적으로 설정하지 않은 경우에만 `{"type": "disabled"}`를 주입합니다.

#### 응답 모델 정규화

`normalize_response_model_identity`가 활성화되면 프록시는 업스트림 응답의 `model` 필드를 다시 씁니다:

- **비스트리밍**: JSON 응답을 구문 분석하고 `model`을 Anthropic 정식 이름으로 다시 쓴 후 다시 직렬화합니다
- **스트리밍 (SSE)**: `message_start` 이벤트 프레임을 가로채고 바이트 범위 교체를 사용하여 `model`을 제자리에서 다시 써서 SSE 형식과 공백을 보존합니다
- **건너뛰는 이유**: `disabled` (토글 끔), `non_success_status` (200이 아닌 응답), `content_encoding_not_transformable` (gzip/brotli), `stream_error`, `stream_cancelled`
- **결정 로직**: 프로덕션 코드와 테스트 모두에서 사용되는 순수 함수 (`should_normalize_nonstream`, `nonstream_skip_reason`)

#### 미디어 확인 / 이미지 정화

모델별 `supports_vision` / `supports_video` 플래그가 동작을 결정합니다. 이미지를 수신하는 비전 미지원 모델의 경우 `non_vision_image_policy`가 적용됩니다:
- `replace` (기본값): 이미지 블록을 플레이스홀더 텍스트로 교체
- `drop`: 이미지 블록 제거 (내용이 비어 있으면 플레이스홀더 삽입)
- `reject`: 400 오류 반환

비디오 블록은 항상 400을 반환합니다. `non_vision_image_policy`는 `/health`를 통해 확인할 수 있습니다.

#### Claude Code 컨텍스트 관리

Claude Code 컨텍스트 제어는 두 개의 공식 환경 변수를 사용합니다:

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

리졸버 파이프라인:

1. 각 표준 라우트(claude-opus-5, claude-sonnet-5, claude-haiku-4-5)를 업스트림 모델로 확인합니다
2. `model_context_windows.json`에서 각 업스트림 모델의 컨텍스트 용량을 조회합니다
3. 세 용량이 모두 알려져 있어야 합니다
4. 알려진 가장 작은 용량을 안전한 컨텍스트 창으로 사용합니다
5. 구성된 트리거 백분율을 적용합니다

모드: `auto` (알려진 가장 작은 용량), `manual` (`window_tokens`), `claude_default` (Claude Code 자체 기본값; 변수 미설정). 유효 상태는 `applied`, `disabled` 또는 `incomplete`입니다.

실행 명령은 게이트웨이 연결 변수와 컨텍스트 변수를 결합합니다:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

컨텍스트 제어가 적용되지 않으면 명령은 먼저 오래된 변수를 제거합니다:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

백분율 오버라이드는 압축을 더 일찍 실행할 뿐입니다. 압축을 Claude Code 기본값보다 늦추는 값은 무시될 수 있습니다. Anthro Bridge는 명령을 생성하고 복사만 합니다 — 실행하지는 않으며, 이는 특정 Claude Code 버전이 해당 변수를 존중한다는 것을 보장하지 않습니다 (최종 확인은 Claude Code 진단 또는 관찰된 압축 동작이 필요합니다).

### 다국어

`import.meta.glob` 자동 탐색이 가능한 언어별 파일 아키텍처:

```
gui/src/i18n/lang/
  en.ts      영어 (정규 — TranslationKey 타입 정의)
  ja.ts      일본어
  zh-CN.ts   중국어(간체)
  zh-TW.ts   중국어(번체)
  ko.ts      한국어
  fr.ts      프랑스어
  de.ts      독일어
  es.ts      스페인어
```

언어 추가: `en.ts`를 복사하고, 번역하고, 다시 빌드하세요. 코드 변경이 필요하지 않습니다.

### config.json 참조

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key env var name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "real-model-name" },
      "visible_models": ["claude-public-model-name"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "real-model-name",
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

각 제공자 또는 OpenRouter 프로필은 `claude_code: { "auto_compact": { "mode": "auto" } }`를 통해 기본 컨텍스트 모드를 설정할 수도 있습니다. 라우트의 유효 모드는 제공자/프로필 값이며, 전역 블록으로 대체됩니다. `resolve_claude_code_auto_compact`는 확인된 결과를 반환합니다.
