[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**현재 릴리스: 0.16.0**

Anthro Bridge는 Claude Desktop 및 Claude Code가 Anthropic 호환 API를 통해 여러 서드파티 LLM 제공업체를 사용할 수 있도록 지원하는 로컬 게이트웨이 및 데스크톱 구성 도구입니다.

이 애플리케이션은 다음 요소로 구성됩니다:

- Rust로 작성된 로컬 프록시 서버
- Tauri 2, React, TypeScript로 구축된 네이티브 Windows GUI
- Anthropic 모델 이름에서 제공업체별 업스트림 모델로의 모델 기반 라우팅
- 경로별 모델, 추론(reasoning) 및 기능 구성

Anthro Bridge는 독립적인 프로젝트입니다. Moon Bridge의 포크, 프론트엔드 또는 보조 애플리케이션이 아닙니다.

## 0.16.0 버전 하이라이트

버전 0.16.0은 모델 인지형(model-aware) Claude Code 컨텍스트 관리를 추가합니다.

- Anthro Bridge는 Opus, Sonnet, Haiku 경로에 할당된 업스트림 모델의 컨텍스트 용량을 해석합니다.
- 자동 모드에서는 세 경로 중 가장 작은 알려진 용량이 안전한 Claude Code 컨텍스트 창으로 사용됩니다.
- 컨텍스트 제어는 세 경로의 용량을 모두 알고 있을 때만 적용됩니다.
- 헤더에 간결한 컨텍스트 관리 토글이 제공되며, 고급 모드 및 임계값은 `config.json`을 통해 계속 사용할 수 있습니다.
- 이 애플리케이션은 Anthro Bridge 연결 변수와 Claude Code 컨텍스트 제어 변수를 포함한 완전한 PowerShell 실행 명령을 생성할 수 있습니다.
- 컨텍스트 관리가 비활성화되거나 불완전한 경우, 생성된 명령은 현재 PowerShell 세션에서 오래된(stale) 컨텍스트 제어 변수를 제거합니다.
- 내장 컨텍스트 메타데이터는 표준 직접 제공업체 모델과 내장 OpenRouter 모델을 포함합니다.
- 생성된 명령과 해당 환경 변수 동작은 Rust 단위 테스트, Windows PowerShell 통합 테스트 및 프론트엔드 복사 흐름 테스트로 검증됩니다.

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
| StepFun Step 3.5 / Step 3.7 | 예 | 지원 시 Low, Medium, High |
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
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
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

프로필은 GUI에서 추가, 이름 변경, 삭제, 드래그 앤 드롭 재정렬, 숨김, 선택이 가능합니다. 대시보드는 표시된 각 프로필당 하나의 카드를 표시하며, 새로고침 후에도 저장된 순서를 유지합니다.

내장 OpenRouter 벤더 그룹에는 현재 Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6 및 기타 인식된 모델 제품군이 포함됩니다. 알 수 없는 모델도 검색 또는 사용자 지정 모델 입력을 통해 사용할 수 있습니다. 대시보드는 라우팅에 전체 ID를 유지하면서 가독성을 위해 `poolside/laguna-s-2.1`과 같은 벤더 한정 ID를 `laguna-s-2.1`로 줄여 표시합니다.

### OpenRouter 가격 및 모델 상세

설정의 모델 가격 패널은 지원되는 OpenRouter 모델의 내장 가격(프롬프트, 출력, 캐시된 입력 가격 포함)을 표시합니다. 프로모션 가격은 GPT-5.6 Sol, Terra, Luna 변형 및 해당 Pro 변형을 포함한 조정된 표준 가격과 함께 표시될 수 있습니다. 가격 비고에는 적용 가능한 경우 장문 컨텍스트(long-context) 가격이 포함될 수 있습니다.

### 반응형 대시보드 크기 조정

초기 창 높이는 3열 대시보드에 표시되는 제공업체 및 OpenRouter 카드 수에서 계산됩니다. 카드 행이 추가되면 네이티브 최소 크기, 모니터 작업 영역, DPI 스케일링 및 제목 표시줄 장식을 존중하면서 창 높이가 늘어납니다. 프로필 표시 여부나 개수가 변경되면 새 행 수에 맞게 높이가 다시 계산되며, 행 수가 변경되지 않는 동안에는 수동 크기 조정이 유지됩니다.

### 지역화된 Windows 설치 프로그램

Windows NSIS 설치 프로그램은 영어, 일본어, 중국어 간체, 중국어 번체, 한국어, 프랑스어, 독일어, 스페인어에 대한 언어 선택을 제공합니다. 설치 프로그램은 Anthro Bridge 애플리케이션 아이콘을 사용하며, 업그레이드 중에 안정적인 사용자 구성을 보존합니다.

### 최신 UI 안정성 개선 사항

구성 쓰기가 직렬화되고, OpenRouter 저장은 오래된 요청 보호가 있는 큐 기반 업데이트 경로를 사용하며, 프로필 재정렬 작업은 새로고침 실패 후에도 깨끗하게 복구됩니다. 회귀 테스트는 프로필 순서, 저장 경합(race), 모델 가격, 대시보드 카드 수 및 창 크기를 다룹니다.

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
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
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

### Claude Code 컨텍스트 관리

Anthro Bridge 0.16.0은 모델 인지형 컨텍스트 설정으로 Claude Code 실행 명령을 생성할 수 있습니다.

해석기(resolver)는 다음 단계를 수행합니다:

1. 각 표준 경로에 할당된 업스트림 모델을 해석합니다:
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. 각 업스트림 모델의 알려진 컨텍스트 용량을 조회합니다.
3. 세 경로의 용량이 모두 알려져 있어야 합니다.
4. 가장 작은 용량을 안전한 컨텍스트 창으로 사용합니다.
5. 구성된 트리거 백분율을 적용합니다.

예를 들어 세 경로가 1,000,000, 262,144, 1,000,000 토큰의 용량으로 해석되면 Anthro Bridge는 다음을 사용합니다:

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

생성된 PowerShell 명령은 공식 Claude Code 변수를 사용합니다:

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

또한 Anthro Bridge 게이트웨이 연결 변수를 포함합니다:

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

예시:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

컨텍스트 관리가 비활성화되거나, Claude Code 기본 동작으로 설정되거나, 경로 용량을 알 수 없어 불완전한 경우, 생성된 명령은 Claude Code를 실행하기 전에 오래된 컨텍스트 변수를 지웁니다:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

백분율 재정의는 더 이른 사전 압축(proactive compaction)을 요청합니다. Claude Code는 자체 기본 동작을 넘어 압축을 지연시키는 값은 무시할 수 있습니다.

Anthro Bridge는 명령 생성과 PowerShell 환경 주입을 검증합니다. 이는 특정 Claude Code 릴리스가 해당 변수를 실제로 사용했음을 스스로 증명하지는 않습니다. 최종 확인은 Claude Code 진단 또는 압축 동작 관찰이 필요합니다.

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

**게이트웨이 시작**을 클릭하세요.

로컬 엔드포인트가 사용 가능한지 확인하세요:

```text
GET http://127.0.0.1:4000/health
```

### 4. Anthro Bridge를 통해 Claude Code 시작

Claude 구성 패널을 열고 **Claude Code 실행 명령 복사**를 클릭하세요.

생성된 명령을 PowerShell에 붙여넣으세요. 이 명령은 다음을 포함합니다:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW` (컨텍스트 관리가 적용될 때)
- `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` (컨텍스트 관리가 적용될 때)
- 오래된 컨텍스트 변수 정리 명령 (컨텍스트 관리가 적용되지 않을 때)

이 명령은 구성된 모델 인지형 컨텍스트 동작을 유지하면서 Anthro Bridge를 게이트웨이로 하여 Claude Code를 실행합니다.

Claude Desktop 및 추가 서드파티 추론 지침은 다음에서 확인할 수 있습니다:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API 엔드포인트

| 메서드 | 경로 | 설명 |
|---|---|---|
| `GET` | `/health` | 게이트웨이 상태 확인 |
| `GET` | `/v1/models` | 공개 경로 모델 목록 |
| `POST` | `/v1/messages` | 스트리밍 및 비스트리밍 Messages API |
| `POST` | `/v1/messages/count_tokens` | 선택한 제공업체가 지원하는 경우 토큰 수 계산 |

## 구성

기본 구성 파일은 `config.json`입니다.

대부분의 설정은 GUI를 통해 변경해야 합니다. 수동 편집은 고급 사용을 위한 것입니다.

주요 모델 필드:

| 키 | 설명 |
|---|---|
| `models.<route>.upstream_model` | 제공업체로 전송되는 업스트림 모델 이름 |
| `models.<route>.thinking_mode` | 경로별 thinking 모드 |
| `models.<route>.reasoning_effort` | 제공업체별 추론 강도 |
| `models.<route>.supports_vision` | 이미지 지원 재정의 |
| `models.<route>.supports_video` | 비디오 지원 재정의 |
| `models.<route>.visible` | 클라이언트와 대시보드에 경로 노출 여부 |
| `non_vision_image_policy` | 지원되지 않는 이미지 입력 처리 방식 |
| `normalize_response_model_identity` | 응답 모델 이름 정규화 여부 |
| `claude_code.auto_compact.enabled` | 전역 컨텍스트 관리 토글 |
| `claude_code.auto_compact.trigger_percent` | 요청된 사전 압축 백분율 |
| `claude_code.auto_compact.mode` | `auto`, `manual` 또는 `claude_default` |
| `claude_code.auto_compact.window_tokens` | `manual` 모드에서 사용되는 수동 컨텍스트 창 |

지원되지 않는 이미지는 다음 정책 중 하나로 처리할 수 있습니다:

- `replace`: 이미지를 텍스트 자리 표시자로 대체
- `drop`: 이미지 콘텐츠 제거
- `reject`: 오류 반환

### 컨텍스트 관리 구성

GUI는 전역 컨텍스트 관리 토글만 표시합니다. 고급 값은 `config.json`에서 직접 편집할 수 있습니다.

자동 모드:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "auto",
      "trigger_percent": 90
    }
  }
}
```

수동 모드:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "manual",
      "window_tokens": 240000,
      "trigger_percent": 90
    }
  }
}
```

Claude Code 기본 동작:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "claude_default"
    }
  }
}
```

`auto` 모드에서 Anthro Bridge는 세 표준 경로 모두에 알려진 컨텍스트 메타데이터가 있을 때만 컨텍스트 변수를 적용합니다. 알 수 없는 사용자 지정 OpenRouter 모델은 유효한 라우팅 대상으로 유지되지만, 메타데이터를 사용할 수 있거나 수동 모드가 구성될 때까지 컨텍스트 관리는 불완전 상태를 보고합니다.

정적 모델 용량은 다음에 저장됩니다:

```text
gui/src-tauri/resources/model_context_windows.json
```

레지스트리에는 내장 프리셋에서 사용하는 표준 DeepSeek, MiniMax, Kimi, MiMo, Poolside, Tencent, InclusionAI, StepFun 및 OpenAI GPT-5.6 모델이 포함됩니다.

## 제공업체 참고 사항

### DeepSeek

`reasoning_effort`(추론 강도):

- `deepseek-v4-pro`（V4-Pro-0813）
  - Normal: 추론 강도 비활성화
  - Thinking: Low / High / Max
- `deepseek-v4-flash`（V4-Flash-0731）
  - Normal: 추론 강도 비활성화
  - Thinking: Low / High / Max

시작 시 DeepSeek V4 Pro 경로에 저장된 레거시 `medium` 또는 `xhigh` 강도는 `high`로 마이그레이션됩니다(DeepSeek의 유효 추론 수준과 일치). 프록시는 전송 전에 강도 값을 정규화(`medium`/`xhigh` → `high`)하고 `output_config.effort` 형식을 사용합니다.

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

내장 OpenAI GPT-5.6 Balanced 프로필은 새 설치 및 새로 생성된 구성에서 모든 경로에 대해 Thinking High를 기본값으로 사용합니다:

- Opus 5 → GPT-5.6 Sol, Thinking, High
- Sonnet 5 → GPT-5.6 Terra, Thinking, High
- Haiku 4.5 → GPT-5.6 Luna, Thinking, High

기존에 저장된 라우팅은 자동으로 변경되지 않습니다.

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
- 헤더의 Claude Code 컨텍스트 관리 토글
- Claude 구성 패널의 Claude Code 실행 명령 복사 작업

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
│   │   │   ├── model_routing.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   │       ├── config.json
│   │       └── model_context_windows.json
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
cargo test
```

컨텍스트 관리 검증은 다음을 다룹니다:

- 프록시와 컨텍스트 해석기 간의 공유된 경로-업스트림 해석
- 내장 직접 제공업체 및 OpenRouter 모델에 대한 완전한 모델 컨텍스트 메타데이터
- 세 표준 경로에 걸친 자동 최소 창 선택
- 적용됨, 비활성화됨, 불완전, 수동 및 Claude 기본 모드
- 공식 Claude Code 환경 변수 이름
- PowerShell 명령 렌더링 및 이스케이프
- 게이트웨이 연결 변수
- 실제 Windows PowerShell 자식 프로세스에서의 환경 주입
- 컨텍스트 관리가 적용되지 않을 때 오래된 컨텍스트 변수 제거
- 생성된 실행 명령의 프론트엔드 복사

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
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
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
- 헤더 컨텍스트 관리 토글이 시각적 스위치를 사용하고 상태를 유지하는지
- 모든 내장 제공업체 또는 OpenRouter 프리셋이 세 경로의 용량을 모두 해석하는지
- 생성된 Claude Code 명령에 게이트웨이 연결 변수가 포함되는지
- 컨텍스트 관리가 활성화된 경우 생성된 명령에 `CLAUDE_CODE_AUTO_COMPACT_WINDOW` 및 `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`가 포함되는지
- 컨텍스트 관리가 비활성화된 경우 생성된 명령이 두 컨텍스트 변수를 모두 제거하는지
- 복사된 명령이 실행 중인 Anthro Bridge 게이트웨이를 통해 Claude Code를 시작하는지

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

### 컨텍스트 관리가 불완전한 경우

자동 컨텍스트 관리는 세 표준 경로 모두에 대해 알려진 용량을 요구합니다.

Opus, Sonnet, Haiku에 대해 구성된 업스트림 모델을 확인하세요. 사용자 지정 또는 새로 출시된 모델은 `model_context_windows.json`에 아직 없을 수 있습니다.

옵션:

1. 알려진 메타데이터가 있는 내장 모델을 선택합니다.
2. 검증된 모델 메타데이터를 정적 레지스트리에 추가합니다.
3. `config.json`에서 수동 모드를 사용합니다.
4. `claude_default`를 사용하여 압축을 전적으로 Claude Code에 맡깁니다.

### Claude Code가 예상한 컨텍스트 설정을 사용하지 않는 경우

Claude Code가 별도의 터미널 명령이 아니라 생성된 PowerShell 명령에서 시작되었는지 확인하세요.

동일한 PowerShell 세션에서 다음을 확인하세요:

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

이 값들은 실행 환경이 준비되었음을 확인합니다. Claude Code가 해당 변수를 사용했다는 것을 증명하지는 않습니다. 최종 확인을 위해 Claude Code 진단을 사용하거나 압축 동작을 관찰하세요.

## 번역

영어가 원본 README입니다.

번역된 README 파일은 `docs/` 아래에 저장됩니다. 영어 README가 변경되면 각 언어를 독립적으로 편집하는 대신 영어 원본에서 번역된 파일을 재생성하거나 업데이트하세요.

애플리케이션 UI의 언어 파일은 다음 위치에 저장됩니다:

```text
gui/src/i18n/lang/
```

## 라이선스

MIT 라이선스. [LICENSE](../LICENSE)를 참조하세요.
