[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## 한국어

### 개요

Anthro Bridge는 여러 제공자의 Anthropic 호환 엔드포인트를 통해 Claude Desktop / Claude Code API 요청을 라우팅하는 프록시 + GUI 관리 도구입니다.

Anthro Bridge는 각 요청의 `model` 필드를 읽고 올바른 업스트림 제공자로 자동 라우팅합니다(모델 기반 라우팅). `model` 필드만 다시 쓰며, messages, thinking 블록, tool_use, tool_result, 스트리밍 SSE는 그대로 전달됩니다.

Anthro Bridge는 Moon Bridge의 포크, GUI 버전 또는 동반 앱이 아닌 독립적인 Anthropic 호환 게이트웨이입니다.

### 지원 제공자

| 제공자 ID | 표시 이름 | 업스트림 엔드포인트 | 기본 모델 |
|-----------|----------|-------------------|----------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

GUI 관리 도구(Tauri v2 + React 19 + TypeScript)는 네이티브 Windows 창에서 시작/중지 제어, 설정 편집, 로그 보기, API 키 관리를 제공합니다.

### 이 게이트웨이가 필요한 이유

Claude Desktop / Claude Code는 기본적으로 Anthropic의 API 형식과 Claude 계열 모델 이름을 기대합니다. DeepSeek, MiniMax, Kimi, MiMo가 Anthropic 호환 API를 제공하더라도 Claude Desktop / Claude Code에서 항상 직접 사용할 수 있는 것은 아닙니다.

특히 **Claude Desktop의 `inferenceModels[].name`은 Anthropic 공식 모델 이름만 허용합니다**. `claude-deepseek-v4`나 `kimi-k2.6` 같은 게이트웨이 사용자 정의 이름은 `"not an Anthropic model"`로 거부됩니다.

이 제한을 우회하기 위해 Anthro Bridge는 **Claude Desktop에 Anthropic 공식 모델 이름(`claude-sonnet-4-6` / `claude-haiku-4-5`)을 "쉘"로 표시하면서 실제 LLM(DeepSeek / MiniMax / Kimi / MiMo)은 GUI에서 선택합니다**.

```
Claude Desktop 측 (항상 고정)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

게이트웨이 내부 (GUI 선택에 따라)
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

이렇게 하면 Claude Desktop의 모델 이름 검증을 통과하면서 DeepSeek, MiniMax, Kimi, MiMo를 자유롭게 전환할 수 있습니다.

### 사전 요구사항

- **Windows 10/11** (일본어 로케이트 지원)
- 선택한 제공자의 API 키 (DeepSeek / MiniMax / Kimi / MiMo — **하나만 있으면 됨**, v0.5.0부터)

### 빠른 시작

#### 1. 설치

[Releases](https://github.com/soheidon/anthro-bridge/releases)에서 최신 설치 프로그램을 다운로드하여 실행합니다.

설치 프로그램은 시작 시 언어 선택 화면을 표시합니다(English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español 중 선택).

#### 업데이트

새 `setup.exe`를 실행하면 됩니다 — 자동으로 이전 버전을 감지하고 교체합니다. 수동 제거는 필요하지 않습니다. 설정(`%APPDATA%\Anthro Bridge\config.json`)은 업데이트 시 유지됩니다.

#### 2. API 키 설정

설정(⚙) → **API 키** 탭에서 제공자의 API 키를 입력하고 **저장**을 클릭합니다.
키는 Windows 사용자 환경 변수로 저장됩니다.

| 제공자 | 환경 변수 | 비고 |
|----------|---------------------|------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. 제공자 선택

대시보드의 **LLM 제공자 선택** 카드는 2×2 그리드로 배치됩니다:

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

타일을 클릭하여 활성 제공자를 선택합니다.

#### 4. 게이트웨이 시작

헤더에서 **Start Gateway**를 클릭합니다. 프록시가 `http://127.0.0.1:4000`에서 시작됩니다.

#### 5. Claude Desktop / Cowork on 3P 구성

자세한 단계별 지침은 [THIRD_PARTY_INFERENCE.ko.md](THIRD_PARTY_INFERENCE.ko.md)를 참조하세요.

### 엔드포인트

| 메서드 | 경로 | 설명 |
|--------|------|------|
| GET | `/health` | 헬스 체크 |
| GET | `/v1/models` | 공개 모델 목록 |
| POST | `/v1/messages` | Messages API (stream + non-stream). 모델 기반 라우팅 |
| POST | `/v1/messages/count_tokens` | 토큰 카운팅 (지원 제공자만) |

### 라우팅

모델 기반 라우팅: 각 요청의 `model` 필드가 대상 제공자와 업스트림 모델을 결정합니다.

| Anthropic 모델 | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|-----------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking 켜짐) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking 꺼짐) | `mimo-v2.5` |

#### MiMo 라우팅 세부사항

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`**: Thinking이 **기본적으로 활성화됨** (`thinking_mode: "thinking"`). MiMo의 thinking 제어에는 `thinking_mode` 키를 사용합니다(`thinking`이 아님). 표준 모드로 전환하려면 `"default"`로 설정.
- **`claude-haiku-4-5` → `mimo-v2.5`**: 이미지 URL 및 base64 이미지 패스스루 지원. Anthro Bridge는 MiMo에서 오디오/비디오 입력을 지원하지 않습니다.
- **`claude-sonnet-4-6` 경로는 이미지를 지원하지 않습니다.** 이 경로로 이미지가 전송되면 텍스트 플레이스홀더로 대체됩니다 (`non_vision_image_policy: "replace"`).
- **업스트림 엔드포인트**: 요청이 `https://api.xiaomimimo.com/anthropic/v1/messages`로 전송됩니다.

### 언어

8개 언어: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español.

새 번역을 추가하려면 언어 파일(예: `es.ts`)을 `gui/src/i18n/lang/`에 넣고 다시 빌드하세요.
자세한 내용은 [CONTRIBUTING](CONTRIBUTING.md)을 참조하세요.

### 구성 (config.json)

제공자 설정은 각 모델의 업스트림 모델 이름과 기능 플래그를 정의합니다. 보통 편집이 필요하지 않습니다.
고급 사용자는 설정(⚙) → **게이트웨이 구성**에서 편집할 수 있습니다.

| 키 | 설명 |
|-----|------|
| `models.<model>.upstream_model` | 업스트림에 보내는 실제 모델 이름 (필수) |
| `models.<model>.thinking` | `"disabled"`일 때 thinking 억제 주입 (선택). MiMo의 경우 `thinking_mode`를 대신 사용 |
| `models.<model>.thinking_mode` | MiMo 전용: `"thinking"`(활성화) 또는 `"default"`(표준). MiMo 제공자에서만 사용 |
| `models.<model>.supports_vision` | 모델별 이미지 지원 (기본값은 제공자 설정으로 복귀) |
| `models.<model>.supports_video` | 모델별 비디오 지원 (기본값은 제공자 설정으로 복귀) |
| `models.<model>.visible` | `/v1/models` 및 대시보드에 표시 여부 (기본값 `true`) |
| `non_vision_image_policy` | 비전 모델의 이미지 처리: `replace`(플레이스홀더) / `drop` / `reject`(오류) |

### 프로젝트 구조

```
anthro-bridge/
├── README.md
├── SPEC.md                    사양
├── docs/
│   ├── README.ja.md           일본어
│   ├── README.zh-CN.md        중국어(간체)
│   ├── README.zh-TW.md        중국어(번체)
│   ├── README.ko.md           한국어
│   ├── README.fr.md           프랑스어
│   ├── README.de.md           독일어
│   ├── README.es.md           스페인어
│   ├── THIRD_PARTY_INFERENCE.md   서드파티 추론 가이드
│   └── THIRD_PARTY_INFERENCE.*.md
├── LICENSE                    MIT 라이선스
├── config.json                제공자 구성
├── .gitignore
└── gui/
    ├── src/                   React 프론트엔드 (TypeScript)
    │   ├── components/        UI 컴포넌트
    │   ├── hooks/             사용자 정의 훅
    │   └── i18n/              다국어 지원
    │       └── lang/          언어 파일 (en, ja, zh-CN, zh-TW, ko, fr, de, es)
    ├── src-tauri/             Tauri 백엔드 (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24개 Tauri 명령 + 프록시 생명주기
    │   │   ├── main.rs        진입점
    │   │   └── proxy.rs       axum 프록시 서버
    │   ├── resources/
    │   │   └── config.json    번들 구성
    │   └── Cargo.toml
    └── package.json
```

### 개발 빌드

```bash
cd gui
npm install
npm run tauri build    # 프로덕션 빌드
npm run tauri dev      # 개발 모드 (HMR)
```

[Rust](https://rustup.rs/) stable 도구 체인과 Node.js 24+가 필요합니다.

### 문제 해결

#### 포트 4000 사용 중

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### 이미지/비디오 거부

DeepSeek는 이미지나 비디오를 지원하지 않습니다. 이미지는 자동으로 플레이스홀더 텍스트로 대체됩니다 (`non_vision_image_policy: "replace"`). 네이티브로 이미지를 사용하려면 MiniMax, Kimi 또는 MiMo(`claude-haiku-4-5` 경로)로 전환하세요.

MiMo의 `claude-sonnet-4-6` 경로도 이미지를 지원하지 않습니다 — 이미지 작업에는 `claude-haiku-4-5`를 사용하세요. 비디오는 항상 거부됩니다.

#### MiMo: 기존 사용자 구성이 변경사항을 반영하지 않음

v0.9.0 이전 버전에서 업그레이드한 경우 저장된 사용자 구성에 여전히 이전 `"display_name": "MiMo"` 또는 `"thinking": "default"` 값이 있을 수 있습니다. v0.9.0은 첫 실행 시 자동으로 마이그레이션하지만 문제가 발생하면:

1. **앱 재시작** — 자동 마이그레이션이 시작 시 실행됩니다.
2. **구성 재설정**: `%APPDATA%\Anthro Bridge\config.json`을 삭제하고 재시작하면 올바른 MiMo 설정이 포함된 번들 구성이 사용됩니다.
3. **수동 확인**: `%APPDATA%\Anthro Bridge\config.json`을 열고 `providers.mimo`에 `"display_name": "MiMo / Xiaomi"`, `"api_key_env": "XIAOMI_API_KEY"`, 모델 항목에 `thinking_mode`(`thinking`이 아닌)가 있는지 확인하세요.

### 수동 테스트 — MiMo / Xiaomi

#### 텍스트 전용 (claude-sonnet-4-6 → mimo-v2.5-pro)

1. 설정 → API 키 탭에서 `XIAOMI_API_KEY`를 설정하고 저장.
2. 대시보드에서 **MiMo / Xiaomi**를 선택.
3. 게이트웨이 시작.
4. Claude Desktop에서 메시지를 보내고 응답이 thinking 블록과 함께 도착하는지 확인.

#### 이미지 테스트 (claude-haiku-4-5 → mimo-v2.5)

1. 대시보드에서 **MiMo / Xiaomi**를 선택.
2. Claude Desktop에서 이미지를 첨부하여 메시지 전송.
3. 이미지가 올바르게 수신되고 설명되는지 확인.
4. `claude-sonnet-4-6`로 이미지를 보내면 텍스트 플레이스홀더로 대체되어야 함.

#### 확인

GUI의 로그 패널을 확인하세요 — 요청의 `model` 필드가 경로에 따라 `mimo-v2.5-pro` 또는 `mimo-v2.5`로 다시 쓰여진 것을 볼 수 있습니다.

### 라이선스

MIT — 자세한 내용은 [LICENSE](LICENSE)를 참조하세요.
