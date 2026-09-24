[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# Claude Desktop / Cowork on 3P에서 Anthro Bridge 사용하기

Anthro Bridge를 Claude Desktop / Cowork on 3P의 로컬 Anthropic 호환 게이트웨이로 사용할 수 있습니다.

Claude Desktop / Cowork on 3P는 앱 내 설정 창을 통해 서드파티 추론을 지원합니다.

공식 문서:

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Anthro Bridge 시작

먼저 Anthro Bridge를 시작하고 게이트웨이가 실행 중인지 확인합니다.

기본적으로 Anthro Bridge는 다음에서 수신합니다:

```text
http://127.0.0.1:4000
```

Claude Desktop / Cowork on 3P를 사용하는 동안 Anthro Bridge를 열어두세요.

## 2. Claude Desktop에서 개발자 모드 활성화

Claude Desktop을 엽니다.

Windows에서는 왼쪽 상단의 애플리케이션 메뉴를 엽니다.

그런 다음 선택합니다:

```text
Help → Troubleshooting → Enable Developer Mode
```

개발자 모드가 활성화되면 새로운 `Developer` 메뉴가 나타납니다.

## 3. 서드파티 추론 설정 열기

다음을 엽니다:

```text
Developer → Configure third-party inference
```

서드파티 추론 설정 창이 열립니다.

## 4. 연결 구성

`Connection` 섹션에서 다음을 선택합니다:

```text
Gateway
```

그런 다음 다음 값을 입력합니다.

| 필드                  | 값                                         |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | 사용자 지정 헤더가 필요하지 않으면 비워두세요 |

`Gateway API key`는 Anthro Bridge에서 구성한 로컬 API 키와 일치해야 합니다.

## 5. 신원 및 모델 구성

`Identity & Models` 섹션에서 Claude Desktop이 모델 선택기에 표시할 모델 ID를 추가합니다.

예시:

```text
claude-sonnet-4-6
claude-haiku-4-5
```

각 모델에 표시 레이블을 지정할 수도 있습니다.

예시:

| 모델 ID              | 표시 레이블       |
| -------------------- | --------------- |
| `claude-sonnet-4-6`  | `Gateway Pro`   |
| `claude-haiku-4-5`   | `Gateway Flash` |

목록의 첫 번째 모델이 선택기의 기본 항목으로 사용됩니다.

각 모델에서 행을 펼치고 `Model ID`가 Claude Desktop이 Anthro Bridge로 보내길 원하는 정확한 모델 이름인지 확인하세요.

업스트림 제공자와 선택된 모델이 확장 컨텍스트 창을 실제로 지원하는 경우에만 `Offer 1M-context variant`를 활성화하세요.

## 6. 구성 예시

위 설정은 다음 서드파티 추론 구성을 나타냅니다:

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-sonnet-4-6",
      "labelOverride": "Gateway Pro"
    },
    {
      "name": "claude-haiku-4-5",
      "labelOverride": "Gateway Flash"
    }
  ]
}
```

## 7. 적용 및 Claude Desktop 재시작

게이트웨이와 모델 목록을 구성한 후 로컬에 설정을 적용합니다.

메시지가 표시되면 Claude Desktop을 재시작하세요.

Claude Desktop이 재시작되면 Cowork on 3P의 요청이 Anthro Bridge로 전송됩니다. Anthro Bridge는 Anthro Bridge에서 구성한 업스트림 제공자로 요청을 라우팅합니다.

## Xiaomi MiMo 공급자

Anthro Bridge가 Direct MiMo 공급자를 사용하도록 구성된 경우, Claude Desktop의 추론 요청은 Xiaomi의 MiMo API로 라우팅됩니다.

### MiMo-V2.6 모델

| Model ID | 표시 이름 | 멀티모달 |
| --- | --- | --- |
| `mimo-v2.6-flash` | MiMo-V2.6-Flash | Image + Video |
| `mimo-v2.6-pro` | MiMo-V2.6-Pro | Image + Video |
| `mimo-v2.6-pro-ultraspeed` | MiMo-V2.6-Pro-UltraSpeed | Image + Video |

모든 V2.6 모델은 1,000,000 토큰 컨텍스트 창을 지원합니다.

### 일반 / 사고

MiMo는 간단한 일반／사고 토글을 사용합니다. 추론 노력 수준(Low / Medium / High)은 없습니다.

| 설정 | 업스트림 파라미터 | 동작 |
| --- | --- | --- |
| 일반 | `thinking: {"type": "disabled"}` | 표준 추론 |
| 사고 | `thinking: {"type": "enabled"}` | 확장 사고 |

### 기본 라우트

Direct MiMo 사전 설정의 내장 라우트:

| 게이트웨이 모델 | 업스트림 모델 | 사고 |
| --- | --- | --- |
| `claude-opus-5` | `mimo-v2.6-pro` | 사고 |
| `claude-sonnet-5` | `mimo-v2.6-pro` | 일반 |
| `claude-haiku-4-5` | `mimo-v2.6-flash` | 사고 |

### V2.5 이전 버전 호환성

저장된 `mimo-v2.5`, `mimo-v2.5-pro`, `mimo-v2.5-pro-ultraspeed` 구성은 보존되어 계속 동작합니다. 시작 시 변경되지 않은 공장 기본 라우트만 자동으로 V2.6으로 마이그레이션됩니다. 사용자 지정 라우트, 사고 모드 설정, 사용자 지정 `default_model` 값은 절대 덮어쓰지 않습니다.

## 참고사항

Anthro Bridge는 비공식 Anthropic 호환 로컬 게이트웨이입니다.

Anthropic, Moon Bridge 또는 어떤 업스트림 모델 제공자와도 관련이 없습니다.

Claude Desktop / Cowork on 3P는 Claude의 서드파티 추론 설정을 통해 구성됩니다. 메뉴 레이블과 구성 필드는 Anthropic이 Claude Desktop을 업데이트함에 따라 변경될 수 있습니다.


> [!NOTE]
> **Claude Desktop / Cowork on 3P는 게이트웨이를 통해 클라우드 제공자를 사용합니다**
>
> Claude Desktop 및 Cowork on 3P는 Anthro Bridge 게이트웨이(`http://127.0.0.1:4000`)로 추론 요청을 보내며, 구성된 상류 클라우드 제공자(DeepSeek, MiMo, Kimi, MiniMax, OpenRouter)로 라우팅됩니다.
>
> **Ollama Local** 백엔드는 **Claude Code CLI 전용**으로 작동하며 Claude Desktop 및 MCP와 완전히 격리되어 있습니다. Claude Code용 로컬 LLM 설정은 [Ollama Local 가이드](OLLAMA_LOCAL.ko.md)를 참조하세요.
