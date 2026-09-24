[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | 한국어 | [Français](OLLAMA_LOCAL.fr.md) | [Deutsch](OLLAMA_LOCAL.de.md) | [Español](OLLAMA_LOCAL.es.md)

[← Anthro Bridge README로 돌아가기](../README.md)

# Claude Code + Ollama Local

Anthro Bridge는 Claude Desktop 및 MCP에서 사용하는 클라우드 공급자 설정을 변경하지 않고, Claude Code만 로컬에서 실행 중인 Ollama 모델로 라우팅할 수 있습니다.

---

## 아키텍처

```text
Claude Code
     │
     │ Anthro Bridge Claude Code 식별 마커 (X-Anthro-Bridge-Client: claude-code)
     ▼
Anthro Bridge Gateway
     │
     ├─ Gateway 라우트 (`active_route = "gateway"`) ──→ 글로벌 클라우드 공급자 (DeepSeek / MiMo / OpenRouter 등)
     │
     └─ Ollama 라우트  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                      로컬 모델
```

Ollama Local은 **Claude Code 전용** 라우트입니다. Claude Desktop 또는 Anthro Bridge MCP 도구에서 사용하는 전역 클라우드 공급자를 대체하지 않습니다.

---

## 요구 사항

1. **Anthro Bridge**가 설치되어 실행 중이어야 합니다.
2. **Ollama**가 로컬 컴퓨터에 설치되어 실행 중이어야 합니다.
3. Ollama에 설치된 모델이 최소 1개 이상 있거나(예: `gemma4:latest`, `llama3.3:70b`, `qwen2.5-coder:32b`), 유효한 커스텀 모델 태그가 있어야 합니다.
4. Anthro Bridge에서 생성된 실행 명령어로 Claude Code를 실행해야 합니다.

---

## Ollama Local 설정

Anthro Bridge를 엽니다:
1. **설정 > API 키**로 이동합니다.
2. API 키 테이블 아래의 **Ollama Local** 설정 카드로 스크롤합니다.

### 기본 설정 행

- **모델**: 감지된 Ollama 모델 또는 사용자 지정 모델을 선택하는 드롭다운.
- **새로고침 (`🔄 새로고침`)**: 로컬 Ollama의 `/api/tags` 엔드포인트를 쿼리하여 설치된 모델 목록을 가져옵니다.
- **Thinking**: **일반 (비활성화)** 또는 **Thinking (활성화)**을 선택합니다.
- **대시보드에 표시**: Ollama Local이 대시보드에 선택 가능한 카드로 표시될지 제어합니다.

### 고급 설정 (`▸ 고급 설정`)

- **엔드포인트**: Ollama 기본 URL (기본값: `http://127.0.0.1:11434`).
- **비전 (Base64)**: 멀티모달 로컬 모델의 이미지 입력을 활성화/비활성화합니다 (기본값: 비활성화).
- **컨텍스트 윈도우**: 선택적 토큰 용량 (예: `131072`).
- **API 키 불필요**: 로컬 루프백 Ollama는 API 키가 필요하지 않습니다.

---

## 모델 검색 및 새로고침

**새로고침 (`🔄 새로고침`)**을 클릭하면 다음으로 요청을 보냅니다:

```http
GET http://127.0.0.1:11434/api/tags
```

### 보안 및 설정 보존 규칙

- **루프백 전용**: 로컬 루프백 주소(`127.0.0.1`, `localhost`, `[::1]`)만 쿼리합니다.
- **비차단 오류 처리**: Ollama가 실행 중이지 않거나 요청 시간이 초과(2초 제한)되어도 알림만 표시되며, 저장된 모델 설정은 **절대 지워지지 않습니다**.
- **사용자 지정 모델 보존**: 저장된 모델이 반환 목록에 없더라도 드롭다운에서 선택된 상태가 유지됩니다.
- **커스텀 태그 수동 입력**: `+ 사용자 지정 모델 태그...`를 선택하여 임의의 모델 태그를 입력할 수 있습니다.

---

## 대시보드에서 Ollama 선택

1. 설정에서 **대시보드에 표시**를 활성화합니다.
2. **대시보드**로 이동합니다.
3. **Ollama Local** 카드를 클릭합니다:
   - `claude_code.active_route = "ollama"`로 설정됩니다.
   - 전역 `active_provider`(예: `deepseek`)는 **변경되지 않습니다**.
   - Ollama 카드가 **Claude Code 사용 중**으로 강조 표시됩니다.
4. 클라우드 공급자 라우트로 복귀하려면:
   - 대시보드에서 클라우드 공급자 카드(DeepSeek, MiMo 등)를 클릭합니다.
   - Claude Code는 자동으로 `claude_code.active_route = "gateway"`로 재설정됩니다.

---

## Thinking 동작 방식

Anthro Bridge는 사용자의 Thinking 선택을 Ollama가 지원하는 Anthropic 호환 형식으로 변환합니다:

- **일반 (Normal)**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking (활성화)**:
  ```json
  "thinking": { "type": "enabled" }
  ```

*참고: Ollama Local은 reasoning effort 레벨(Low/High/Max)을 제공하지 않습니다.*

---

## 컨텍스트 윈도우 및 자동 압축

- `context_window`는 **선택 사항**입니다.
- 지정된 경우 Anthro Bridge는 이를 Claude Code 컨텍스트 관리 및 auto-compact 계산에 사용합니다.
- 생략(`null`)된 경우 Anthro Bridge는 임의의 크기를 추측하지 않고 기본 규칙을 따릅니다.

---

## Claude Code 식별 마커

Anthro Bridge가 생성하는 Claude Code 실행 명령어는 `ANTHROPIC_CUSTOM_HEADERS`를 통해 내부 마커를 주입합니다:

```text
X-Anthro-Bridge-Client: claude-code
```

- **헤더 정규화**: 대소문자를 구분하지 않고 정규화하며, 중복되거나 오래된 마커를 단일 표준 마커로 정리하고 사용자의 기타 사용자 지정 헤더를 보존합니다.
- **업스트림 전달 전 제거**: 이 마커는 Anthro Bridge 로컬 라우팅 판단에만 사용되며, 업스트림 공급자나 Ollama로 전달되기 전에 완전히 제거됩니다.

---

## 클라이언트 격리

| 클라이언트 | 라우트 선택자 | 대상 |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` 또는 `"ollama"`) | 대시보드에서 선택된 공급자 / Ollama Local |
| **Claude Desktop** | `active_provider` (글로벌 클라우드 공급자) | 클라우드 게이트웨이 (DeepSeek, MiMo, OpenRouter 등) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | 전용 MCP 계획/검토 공급자 |

---

## 문제 해결

### 새로고침 시 Ollama를 찾을 수 없음
1. 터미널 또는 백그라운드에서 Ollama가 실행 중인지 확인합니다:
   ```powershell
   ollama list
   ```
2. 고급 설정의 엔드포인트가 `http://127.0.0.1:11434`인지 확인합니다.

### 저장된 모델이 목록에 없음
`/api/tags` 목록에 없더라도 Anthro Bridge는 설정된 모델을 유지합니다. `+ 사용자 지정 모델 태그...`를 통해 수동으로 입력할 수도 있습니다.

### Claude Code가 여전히 클라우드 공급자를 사용함
1. 대시보드에서 **Ollama Local** 카드가 활성화되어 있는지 확인하세요.
2. Anthro Bridge에서 생성된 명령어(`Claude Code 실행 명령어 복사`)로 실행했는지 확인하세요.

### Claude Desktop이 여전히 클라우드 공급자를 사용함
정상적인 동작입니다. Ollama Local은 Claude Code 전용입니다.
