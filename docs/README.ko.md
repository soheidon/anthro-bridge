[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

Anthro Bridge는 Claude Desktop 및 Claude Code가 Anthropic 호환 API를 통해 여러 서드파티 LLM 제공업체를 사용할 수 있도록 지원하는 로컬 게이트웨이 및 데스크톱 구성 도구입니다.

이 애플리케이션은 다음 요소로 구성됩니다:

- Rust로 작성된 로컬 프록시 서버
- Tauri 2, React, TypeScript로 구축된 네이티브 Windows GUI
- Anthropic 모델 이름에서 제공업체별 업스트림 모델로의 모델 기반 라우팅
- 경로별 모델, 추론(reasoning) 및 기능 구성

Anthro Bridge는 독립적인 프로젝트입니다. Moon Bridge의 포크, 프론트엔드 또는 보조 애플리케이션이 아닙니다.

## 지원 모델

Anthro Bridge는 두 가지 범주의 업스트림 모델을 지원합니다.

### 네이티브 통합

이 제공업체들은 자체 Anthropic 호환 API를 통해 지원됩니다. OpenRouter 계정이 필요하지 않습니다.

| 제공업체 | 지원 모델 제품군 | 연결 방식 |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro 및 V4 Flash | 직접 제공업체 API |
| MiniMax | MiniMax M3 및 M2.7 변형 | 직접 제공업체 API |
| Kimi / Moonshot | Kimi K2.x 및 Kimi K3 | 직접 제공업체 API |
| MiMo / Xiaomi | MiMo V2.5 및 V2.5 Pro 변형 | 직접 제공업체 API |

### OpenRouter를 통해 지원되는 모델

이 모델들은 OpenRouter 프로필을 통해 접근합니다. 각 프로필은 자체 API 키, 경로 매핑 및 추론 설정을 가집니다.

| 벤더 또는 모델 제품군 | 내장 지원 | 추론 제어 |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | 예 | 모델별 Thinking 제어 |
| Tencent Hy3 | 예 | Low 및 High 추론 강도 |
| InclusionAI Ring | 예 | 모델별 Thinking 및 추론 제어 |
| StepFun Step 3.5 / Step 3.7 | 예 | Low, Medium, High (지원 시) |
| InclusionAI Ling 제품군 | 예 | 모델별 Thinking 제어 |
| OpenAI GPT-5.6 Sol / Terra / Luna | 예 | 모델별 Thinking 및 추론 제어 |

기타 OpenRouter 모델도 실시간 OpenRouter 모델 목록에서 선택하거나 수동으로 입력할 수 있습니다. 내장 지원이란 Anthro Bridge가 이미 모델 제품군, 기능 플래그, 벤더 그룹화 및 추론 제어 동작을 알고 있음을 의미합니다.

## 작동 방식

Claude Desktop 및 Claude Code는 다음과 같은 Anthropic 모델 이름을 사용하여 요청을 보냅니다:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge는 이러한 이름을 안정적인 경로 식별자로 취급합니다. GUI는 각 경로가 사용할 제공업체와 업스트림 모델을 결정합니다.

예시:

```text
Claude Code 요청
  model: claude-sonnet-5

Anthro Bridge 경로
  provider: OpenRouter 프로필 "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

업스트림 제공업체에 맞게 조정해야 하는 필드만 변경됩니다. 메시지, 도구 호출, 도구 결과, thinking 블록 및 스트리밍 데이터는 업스트림 API가 지원하는 한 그대로 보존됩니다.

## 주요 기능

### 제공업체 라우팅

Anthro Bridge는 두 가지 업스트림 연결 유형을 지원합니다:

1. **직접 제공업체 통합**: 제공업체의 자체 Anthropic 호환 API에 연결합니다.
2. **OpenRouter 프로필**: OpenRouter에 연결하여 단일 API를 통해 여러 벤더 및 모델 제품군으로 라우팅할 수 있습니다.

#### 직접 제공업체 통합

| 제공업체 ID | 표시 이름 | 기본 엔드포인트 |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter 통합

| 연결 유형 | 표시 이름 | 엔드포인트 |
|---|---|---|
| 다중 프로필 모델 게이트웨이 | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter는 단일 모델 제공업체로 취급되지 않습니다. 각 OpenRouter 프로필은 Poolside, Tencent, InclusionAI, StepFun과 같은 지원 벤더 그룹의 모델 및 OpenRouter API에서 검색되거나 수동으로 입력된 기타 모델을 독립적으로 선택할 수 있습니다.

각 Anthropic 경로는 직접 제공업체 모델 또는 OpenRouter 프로필을 통해 선택된 모델에 독립적으로 매핑될 수 있습니다.

### OpenRouter 다중 프로필 지원

여러 OpenRouter 프로필을 독립적으로 생성하고 관리할 수 있습니다.

각 프로필은 다음을 개별적으로 가집니다:

- 프로필 이름
- API 키 구성
- Opus, Sonnet, Haiku 경로 매핑
- Thinking 또는 추론 설정
- 캐시된 OpenRouter 모델 목록

프로필은 GUI에서 추가, 이름 변경, 삭제 및 선택할 수 있습니다.

내장 OpenRouter 벤더 그룹에는 현재 Poolside, Tencent, InclusionAI, StepFun 및 기타 인식된 모델 제품군이 포함됩니다. 알 수 없는 모델도 검색 또는 사용자 지정 모델 입력을 통해 사용할 수 있습니다.

### 모델 및 추론 제어

사용 가능한 제어 옵션은 선택한 모델에 따라 다릅니다.

지원되는 제어 옵션은 다음과 같습니다:

- Thinking 켜기 또는 끄기
- Normal, low, medium, high, xhigh 또는 max 추론 모드
- 제공업체별 추론 강도
- 사용자 선택을 허용하지 않는 모델을 위한 고정 추론 모드

모델을 전환할 때 Anthro Bridge는 가장 가까운 호환 추론 설정을 유지하려고 시도합니다. 이전 설정을 정확히 사용할 수 없는 경우, 가장 가까운 지원 옵션을 선택하며, 두 옵션이 동일하게 가까운 경우 더 낮은 옵션을 우선합니다.

### 기능 감지

Anthro Bridge는 내장 기능 레지스트리와 실시간 OpenRouter 메타데이터를 결합합니다.

감지 가능한 기능:

- 이미지 입력
- 비디오 입력
- Thinking 지원
- 추론 강도 지원
- 알려진 가격
- 제공업체별 요청 변환 규칙

실시간 OpenRouter 메타데이터는 불필요한 API 호출을 줄이기 위해 캐시됩니다.

### 응답 모델 정규화

업스트림 API는 종종 응답에 자체 모델 이름을 반환합니다. Anthro Bridge는 이 필드를 클라이언트가 기대하는 Anthropic 경로 이름으로 다시 작성할 수 있습니다.

예시:

```text
업스트림 응답 모델: deepseek-v4-pro
클라이언트 표시 모델:  claude-sonnet-5
```

정규화는 스트리밍 및 비스트리밍 응답 모두에 적용되며, 설정에서 활성화 또는 비활성화할 수 있습니다.

### 직렬화된 구성 쓰기

구성 변경은 동시 쓰기로 인한 설정 손상이나 되돌림을 방지하기 위해 직렬화됩니다.

다음 작업이 해당됩니다:

- 모델 변경
- Thinking 모드 변경
- 추론 강도 변경
- OpenRouter 프로필 변경
- API 키 관련 구성 변경

### OpenRouter 저장 큐

OpenRouter 경로 변경은 전용 저장 큐를 통해 처리됩니다.

이 큐는 다음을 제공합니다:

- 직렬화된 저장 작업
- 오래된 요청의 대체
- 요청 제출 시점에 캡처된 경로 식별
- 오래된 React 클로저(stale closure)로부터의 보호
- 이전에 선택된 경로로의 롤백 방지
- 저장 성공 후 새로고침 재시도
- 집계된 게이트웨이 재시작 처리
- 저장 후 작업 중 추가된 요청의 안전한 처리

이로 인해 빠른 모델 변경, 경로 전환 또는 지연된 Tauri 응답이 이전 UI 값을 복원하는 것을 방지합니다.

### 게이트웨이 관리

GUI는 다음을 제공합니다:

- 게이트웨이 시작 및 중지 제어
- 제공업체 및 프로필 선택
- 경로 구성
- API 키 관리
- 로그 보기
- 모델 목록 새로고침
- 저장 상태 및 오류 표시

게이트웨이는 다음에서 수신 대기합니다:

```text
http://127.0.0.1:4000
```

## 요구 사항

- Windows 10 또는 Windows 11
- 개발용 Node.js 24 이상
- 개발용 안정 Rust 툴체인
- 지원되는 제공업체 중 하나 이상의 API 키

단일 제공업체 키로 충분합니다. 모든 제공업체의 키가 필요하지는 않습니다.

## 설치

프로젝트 Releases 페이지에서 최신 Windows 설치 프로그램을 다운로드하여 실행하세요.

설치 프로그램은 다음 언어를 지원합니다:

- 영어
- 일본어
- 중국어 간체
- 중국어 번체
- 한국어
- 프랑스어
- 독일어
- 스페인어

Anthro Bridge를 업데이트하려면 최신 설치 프로그램을 실행하세요. 기존 사용자 설정은 유지됩니다.

안정 버전 사용자 구성은 다음 위치에 저장됩니다:

```text
%APPDATA%\Anthro Bridge\
```

개발 빌드는 별도의 애플리케이션 식별자와 데이터 디렉터리를 사용합니다:

```text
%APPDATA%\Anthro Bridge Dev\
```

이를 통해 안정 버전과 개발 버전이 구성 파일이나 캐시 파일을 공유하지 않고 공존할 수 있습니다.

## 빠른 시작

### 1. API 키 구성

열기:

```text
Settings > API Key
```

사용할 제공업체의 키를 입력하고 저장하세요.

일반적인 환경 변수 이름:

| 제공업체 | 환경 변수 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter 프로필은 GUI를 통해 관리되는 프로필별 키 설정을 사용할 수 있습니다.

### 2. 경로 모델 구성

설정을 열고 각 경로에 대한 업스트림 모델을 선택하세요:

- Opus
- Sonnet
- Haiku

OpenRouter의 경우 먼저 프로필을 선택하거나 생성한 다음, 해당 프로필 내에서 각 경로를 구성하세요.

### 3. 게이트웨이 시작

**Start Gateway**를 클릭하세요.

로컬 엔드포인트가 사용 가능한지 확인하세요:

```text
GET http://127.0.0.1:4000/health
```

### 4. Claude Desktop 또는 Claude Code 구성

Anthropic 모델 이름을 계속 사용하면서 클라이언트를 Anthro Bridge 엔드포인트로 지정하세요.

자세한 서드파티 추론 지침은 다음에서 확인할 수 있습니다:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API 엔드포인트

| Method | Path | 설명 |
|---|---|---|
| `GET` | `/health` | 게이트웨이 상태 확인 |
| `GET` | `/v1/models` | 공개 경로 모델 목록 |
| `POST` | `/v1/messages` | 스트리밍 및 비스트리밍 Messages API |
| `POST` | `/v1/messages/count_tokens` | 선택한 제공업체가 지원 시 토큰 수 계산 |

## 구성

기본 구성 파일은 `config.json`입니다.

대부분의 설정은 GUI를 통해 변경해야 합니다. 수동 편집은 고급 사용자를 위한 것입니다.

주요 모델 필드:

| 키 | 설명 |
|---|---|
| `models.<route>.upstream_model` | 제공업체로 전송되는 업스트림 모델 이름 |
| `models.<route>.thinking_mode` | 경로별 thinking 모드 |
| `models.<route>.reasoning_effort` | 제공업체별 추론 강도 |
| `models.<route>.supports_vision` | 이미지 지원 재정의 |
| `models.<route>.supports_video` | 비디오 지원 재정의 |
| `models.<route>.visible` | 클라이언트와 대시보드에 경로 표시 여부 |
| `non_vision_image_policy` | 지원되지 않는 이미지 입력 처리 방식 |
| `normalize_response_model_identity` | 응답 모델 이름 정규화 여부 |

지원되지 않는 이미지는 다음 정책 중 하나로 처리할 수 있습니다:

- `replace`: 이미지를 텍스트 자리 표시자로 대체
- `drop`: 이미지 콘텐츠 제거
- `reject`: 오류 반환

## 제공업체 참고 사항

### DeepSeek

`reasoning_effort`(추론 강도):

- `deepseek-v4-pro`
  - Normal: 추론 강도 비활성화
  - Thinking: High / Max
- `deepseek-v4-flash`
  - Normal: 추론 강도 비활성화
  - Thinking: Low / High / Max

시작 시 DeepSeek V4 Pro 라우트에 저장된 레거시 `low` 또는 `medium` 강도는 `high`로 마이그레이션됩니다(공식 유효 수준과 일치).

새 설치 및 새로 생성된 구성의 기본 DeepSeek 라우팅:

- Opus 5 → V4 Flash, Thinking, Max
- Sonnet 5 → V4 Flash, Thinking, High
- Haiku 4.5 → V4 Flash, Thinking, Low

기존에 저장된 라우팅은 자동으로 변경되지 않습니다.

### MiniMax

MiniMax 모델 동작은 모델 세대에 따라 다릅니다. Anthro Bridge는 선택한 모델에 필요한 요청 형식을 적용하며, 지원 시 적응형 또는 비활성화된 thinking을 포함합니다.

### Kimi

Kimi 모델은 모델 제품군에 따라 thinking 매개변수 또는 고정 추론 강도 모드를 사용할 수 있습니다. Anthro Bridge는 GUI 선택을 적절한 업스트림 요청 형식으로 변환합니다.

### MiMo

MiMo는 지원되는 경로에 대해 일반 `thinking` 필드 대신 `thinking_mode`를 사용합니다.

비전 지원은 모델별로 다릅니다. Anthro Bridge는 경로가 이미지 입력을 수락할 수 없는 경우 구성된 미지원 이미지 정책을 적용합니다.

### OpenRouter

OpenRouter 모델은 인식될 경우 벤더별로 그룹화됩니다. GUI는 다음을 제공합니다:

- 모델 검색
- 벤더 그룹화
- 사용자 지정 모델 입력
- 기능 배지
- 가격 표시
- 모델별 추론 제어
- 통합 모델 목록 새로고침

OpenRouter 모델 기능과 동작은 시간이 지남에 따라 변경될 수 있습니다. 실시간 메타데이터는 가능한 경우 사용되며, 내장 레지스트리는 알려진 모델에 대한 안정적인 기본값을 제공합니다.

## 사용자 인터페이스

설정 인터페이스는 다음을 포함합니다:

- 접이식 제공업체 섹션
- Opus, Sonnet, Haiku 경로 구성
- OpenRouter용 모델 검색 및 벤더 그룹화
- 모델 기능에 기반한 Thinking 및 추론 제어
- 사용자 지정 업스트림 모델 입력
- 자동 경로 저장
- 명시적 API 키 저장
- 저장 진행 상황 및 오류 메시지
- 모델 가격 및 기능 정보
- 응답 모델 정규화 토글

대시보드는 다음을 포함합니다:

- 제공업체 또는 OpenRouter 프로필 선택
- 게이트웨이 상태
- 현재 경로 매핑
- 기능 표시기
- 가격 정보
- 제공업체 전환 상태

## 개발

### 프로젝트 구조

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   └── package.json
└── LICENSE
```

### 개발 모드에서 실행

```bash
cd gui
npm install
npm run tauri dev
```

### 개발 변형 빌드

Windows에서는 간헐적인 컴파일러 종료를 방지하기 위해 단일 Rust 빌드 작업을 사용하세요:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

개발 빌드는 다음을 사용합니다:

- 창 제목: `Anthro Bridge (DEV)`
- 포트: `4000`
- 애플리케이션 식별자: `com.soheidon.anthro-bridge.dev`
- 별도의 구성 및 캐시 디렉터리

### 안정 빌드

안정 빌드는 릴리스 준비용으로만 생성해야 합니다. 일반적인 구현 및 확인 작업은 개발 변형을 사용해야 합니다.

## 검증

프론트엔드 검증:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rust 검증:

```bash
cd gui/src-tauri
cargo check
```

OpenRouter 경로 선택기 특화 검증:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

OpenRouter 선택기 테스트는 다음을 다룹니다:

- 큐에 저장된 저장 중 캡처된 경로 식별
- 경로 간 롤백 방지
- 오래된 콜백(stale callback) 방지
- 새로고침 재시도 동작
- 새로고침 실패 후 게이트웨이 재시작
- 진행 중 요청 대체
- 세대 기반 롤백 억제

재시작 집계를 위한 전용 다중 저장 테스트가 다음 동작을 확정하기 위해 추가될 수 있습니다:

```text
save 1이 재시작 요청
save 2는 재시작 요청하지 않음
결과: 배치 후 한 번만 재시작
```

## 수동 검증 체크리스트

자동화된 테스트는 모든 Tauri 및 React 타이밍 조건을 재현하지 않습니다. 릴리스 전에 개발 빌드에서 다음을 확인하세요:

- 각 OpenRouter 프로필이 올바른 호버 세부 정보를 표시하는지
- 모델 선택이 변경 후 눈에 띄게 되돌아가지 않는지
- Thinking 및 추론 선택이 저장 후 안정적으로 유지되는지
- 설정 화면을 닫고 다시 열어도 설정이 올바르게 유지되는지
- 애플리케이션을 다시 시작해도 설정이 올바르게 유지되는지
- 저장 중 프로필을 전환해도 어느 프로필도 손상되지 않는지
- 저장 실패 시 해당 경로만 롤백되는지
- 새로고침 재시도 성공 시 이전 오류가 지워지는지
- 새로고침 재시도 실패 시 최신 오류가 표시되는지
- 필요한 게이트웨이 재시작이 배치 후 한 번만 발생하는지
- 사용자 지정 모델이 올바르게 저장되고 다시 로드되는지
- 내장 및 실시간 OpenRouter 기능이 올바르게 표시되는지

## 문제 해결

### 포트 4000이 이미 사용 중인 경우

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 모델이 이미지 또는 비디오 입력을 거부하는 경우

모델 기능은 제공업체와 경로에 따라 다릅니다. GUI에서 기능 배지를 확인하고 호환되는 경로를 선택하세요.

지원되지 않는 이미지 입력의 경우, Anthro Bridge는 `non_vision_image_policy`를 따릅니다.

### 업그레이드 후 설정이 되돌려지는 경우

마이그레이션이 실행될 수 있도록 먼저 애플리케이션을 다시 시작하세요.

문제가 지속되는 경우:

1. 사용자 구성을 백업하세요.
2. 번들 구성과 비교하세요.
3. 더 이상 사용되지 않는 필드를 제거하거나 필요한 경우 사용자 구성을 초기화하세요.

안정 버전 구성 위치:

```text
%APPDATA%\Anthro Bridge\config.json
```

개발 버전 구성 위치:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter 모델 목록이 오래된 경우

설정에서 통합 모델 새로고침 컨트롤을 사용하세요. Anthro Bridge는 모델 메타데이터를 캐시하므로, OpenRouter가 모델 항목을 변경한 후 수동 새로고침이 필요할 수 있습니다.

## 번역

영어가 원본 README입니다.

번역된 README 파일은 `docs/` 아래에 저장됩니다. 영어 README가 변경되면 각 언어를 독립적으로 편집하는 대신 영어 원본에서 번역된 파일을 재생성하거나 업데이트하세요.

애플리케이션 UI의 언어 파일은 다음 위치에 저장됩니다:

```text
gui/src/i18n/lang/
```

## 라이선스

MIT 라이선스. [LICENSE](LICENSE)를 참조하세요.
