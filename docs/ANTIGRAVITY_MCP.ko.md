[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Anthro Bridge README로 돌아가기](README.ko.md)

# Google Antigravity에서 Anthro Bridge MCP 사용하기

Anthro Bridge에는 특화된 `plan` 도구(`anthro-bridge/plan`)를 제공하는 Model Context Protocol (MCP) 서버가 내장되어 있습니다. 이를 통해 Google Antigravity와 같은 에이전트 환경에서 아키텍처 설계 및 구현 계획 수립을 외부 LLM(DeepSeek V4, MiMo, Kimi, MiniMax 또는 OpenRouter 모델 등)에 위임하면서, 실제 토큰 소모가 많은 코드 수정, 명령어 실행, 빌드, 테스트는 Antigravity의 구독 모델 용량으로 수행할 수 있습니다.

---

## 1. 이 워크플로의 작동 방식

```text
Antigravity
    ↓
저장소 탐색 (관련 파일 검사 및 컨텍스트 수집)
    ↓
anthro-bridge / plan (작업, 컨텍스트, 제약 조건 전달)
    ↓
Anthro Bridge MCP 서버
    ↓
외부 플래너 모델 (Anthro Bridge GUI에서 설정)
    ↓
구조화된 구현 계획 반환
    ↓
Antigravity가 구독 용량으로
파일 편집, 빌드, 테스트 수행
```

- **외부 API**: 관련 저장소 컨텍스트를 기반으로 구현 계획을 생성하는 역할만 담당합니다(해당 제공업체에서 종량제 과금).
- **Antigravity 구독**: 대규모 파일 읽기/쓰기, 코드 수정, 도구 호출, 테스트 실행 루프를 담당합니다.
- **역할 분리**: 고지능 외부 모델의 계획 수립 능력을 활용하면서도 일상적인 코드 생성에 외부 API 토큰을 낭비하지 않습니다.

---

## 2. 사전 요구 사항

1. Windows에 **Anthro Bridge**가 설치되어 있어야 합니다.
2. **`anthro-bridge-mcp-server.exe`**가 빌드되어 있거나 배치되어 있어야 합니다(예: `mcp-server/target/release/anthro-bridge-mcp-server.exe`).
3. 사용할 플래너 모델의 **API 키**가 구성되어 있어야 합니다.
4. **Google Antigravity**가 설치되어 실행 중이어야 합니다.

---

## 3. Antigravity에 MCP 서버 구성하기

1. Google Antigravity를 엽니다.
2. 다음 경로로 이동합니다:
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. `mcpServers` 객체에 `anthro-bridge` 서버 구성을 추가합니다:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> MCP 구성 파일에 API 키를 일반 텍스트로 적을 필요가 없습니다. MCP 서버는 기존 Windows 사용자 환경 변수(`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` 등) 또는 Anthro Bridge 구성에서 API 키를 자동으로 읽어옵니다.

---

## 4. MCP 연결 확인하기

Antigravity의 **Installed MCP Servers** 보기에서 `anthro-bridge`가 인식되었는지 확인합니다:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Anthro Bridge에서 플래너 모델 설정하기

1. **Anthro Bridge** 데스크톱 앱을 엽니다.
2. 상단의 **MCP** 탭을 선택합니다.
3. 활성화할 플래너 **제공업체 (Provider)** 또는 **프로필 (Profile)**(예: DeepSeek, MiMo, OpenRouter 등)을 선택합니다.
4. **설정 (Settings)**(또는 MCP Plan 상세 설정)을 열고 다음 항목을 구성합니다:
   - **모델 (Model)**
   - **Thinking 모드**
   - **추론 강도 (Reasoning Effort)**
5. 설정을 저장합니다.

> [!NOTE]
> Anthro Bridge MCP 서버는 각 `plan()` 도구 호출 시 현재 구성을 동적으로 읽습니다. GUI에서 플래너 제공업체나 모델을 변경하더라도 MCP 서버나 Antigravity를 다시 시작할 필요가 없습니다.

---

## 6. 수동으로 plan 도구 호출하기

Antigravity 채팅에서 플래너 호출을 직접 요청할 수 있습니다:

```text
이 프로젝트를 조사한 후 anthro-bridge/plan MCP 도구를 사용하여 구현 계획을 수립하세요. 아직 구현하지는 마세요.
```

Antigravity가 관련 파일을 조사하고 컨텍스트를 정리한 후 `anthro-bridge/plan`을 호출하여 검토할 구현 계획을 제시합니다.

---

## 7. Workspace Rule로 계획 수립 자동화하기

[`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md)와 같은 작업 공간 규칙 파일을 생성하여 복잡한 코딩 작업 시 플래너 호출을 자동화할 수 있습니다:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. 일반적인 자동화 워크플로

```text
사용자: "다중 프로필을 지원하도록 기능 X를 리팩토링해줘."
    ↓
Antigravity가 코드를 조사하고 컨텍스트 요약
    ↓
Antigravity가 자동으로 anthro-bridge/plan 도구 호출
    ↓
Anthro Bridge가 선택된 외부 모델로 프롬프트 전송
    ↓
Antigravity가 구조화된 구현 계획 수신
    ↓
사용자가 계획 검토 및 승인
    ↓
Antigravity가 파일 수정 및 테스트 실행
```

---

## 9. 중요 참고 사항

- **독립적인 작동**: MCP 서버는 Anthro Bridge 3P Gateway와 독립적으로 작동합니다. MCP 도구를 사용하기 위해 3P Gateway가 실행 중일 필요는 없습니다.
- **과금 분리**: `anthro-bridge/plan` 호출에는 선택한 제공업체의 API 비용이 발생합니다. 이후의 파일 수정 및 테스트는 Antigravity 자체의 구독 용량을 사용합니다.
- **실시간 반영**: Anthro Bridge GUI에서 플래너 설정을 변경하면 다음 `plan()` 호출 시 즉시 적용됩니다.
