[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Anthro Bridge README로 돌아가기](../README.ko.md)

# Google Antigravity에서 Anthro Bridge MCP 사용하기

Anthro Bridge는 별도의 독립된 MCP 서버 실행 파일을 필요로 하지 않습니다. 설치된 단일 `anthro-bridge.exe`가 데스크톱 GUI 애플리케이션과 MCP 서버 기능을 모두 제공합니다. Antigravity는 동일한 실행 파일을 `--mcp-server` 인자와 함께 실행하여 MCP 모드로 진입합니다.

```text
일반 실행
anthro-bridge.exe
→ Anthro Bridge 데스크톱 앱 / 3P Gateway

MCP 실행
anthro-bridge.exe --mcp-server
→ Antigravity용 헤드리스 stdio MCP 서버
```

이를 통해 Google Antigravity와 같은 에이전트 환경에서 아키텍처 설계 및 구현 계획 수립을 외부 LLM(DeepSeek V4, MiMo, Kimi, MiniMax 또는 OpenRouter 모델 등)에 `anthro-bridge/plan`을 통해 위임하면서, 실제 토큰 소모가 많은 코드 수정, 명령어 실행, 빌드, 테스트는 Antigravity의 구독 모델 용량으로 수행할 수 있습니다.

---

## 1. 이 워크플로의 작동 방식

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
설정된 외부 플래너 모델
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
2. 플래너로 사용하려는 제공업체의 인증 정보(Anthro Bridge 내부 설정 또는 시스템 환경 변수)가 구성되어 있어야 합니다.
3. **Google Antigravity**가 설치되어 실행 중이어야 합니다.

---

## 3. Antigravity에 MCP 서버 구성하기

### 방법 1 — Anthro Bridge GUI를 통한 구성 (권장)

1. Anthro Bridge를 열고 상단 탐색의 **설정**(`[설정]` 탭) > 좌측 하위 탐색의 **Antigravity**를 선택합니다.
2. **Google Antigravity 연동** 카드를 확인합니다:
   - **대상 실행 파일**: 기본적으로 현재 실행 중인 `anthro-bridge.exe` 경로가 표시됩니다. 포터블 버전이나 커스텀 빌드 등 다른 바이너리를 사용하려면 **변경** (`antigravity.btnChangeExe`) 버튼을 눌러 선택합니다.
   - **등록 / 업데이트**: **Antigravity 설정 업데이트** (`antigravity.btnUpdate`)를 클릭하면 `%USERPROFILE%\.gemini\config\mcp_config.json` 내의 다른 MCP 서버 항목을 그대로 보존하면서 `anthro-bridge`를 안전하게 등록하거나 업데이트합니다.
   - **등록 해제**: Antigravity에서 등록을 제거하려면 **설정 제거** (`antigravity.btnRemove`)를 클릭합니다.
   - **설정 폴더 확인**: **설정 폴더 열기** (`antigravity.btnOpenFolder`)를 클릭하여 Windows 파일 탐색기에서 해당 폴더를 직접 열 수 있습니다.

---

### 방법 2 — 수동 구성 (고급)

1. Anthro Bridge **설정 > Antigravity**에서 **설정 폴더 열기**를 클릭하여 Windows 파일 탐색기에서 `%USERPROFILE%\.gemini\config\`를 엽니다.
2. `mcp_config.json`을 열고 `mcpServers` 객체에 `anthro-bridge` 구성을 추가합니다:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

개발 릴리스 빌드 바이너리의 경우 Release 경로를 지정할 수 있습니다:
```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

> [!TIP]
> Antigravity의 `mcp_config.json`에 제공업체 API 키를 적을 필요가 없습니다. MCP 서버는 Anthro Bridge의 기존 자격 증명 확인 메커니즘(Windows 사용자 환경 변수인 `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` 또는 저장된 애플리케이션 설정)을 자동으로 활용합니다.

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

Anthro Bridge는 플래너 선택과 상세 파라미터 관리의 역할을 명확히 구분합니다:

1. **최상위 `MCP` 탭 (`MCP for Antigravity`)**:
   - 사용 가능한 제공업체(DeepSeek, OpenRouter, MiniMax, MiMo, Kimi) 및 프로필 카드 목록이 표시됩니다.
   - 카드를 클릭하여 활성 플래너 대상을 즉시 전환합니다.
2. **`설정` > `Antigravity`**:
   - **MCP Plan 상세 설정** 카드: 제공업체/프로필별로 모델, Thinking 모드, 추론 강도(Reasoning Effort)를 상세히 설정합니다.
   - **Google Antigravity 연동** 카드: MCP 서버 등록 상태 및 Antigravity Commands(전역 스킬)를 관리합니다.

> [!NOTE]
> Anthro Bridge MCP 서버는 각 `plan()` 도구 호출 시 현재 구성을 동적으로 읽습니다. GUI에서 플래너 제공업체나 모델 설정을 변경하더라도 MCP 서버나 Antigravity를 다시 시작할 필요가 없습니다.

---

## 6. Antigravity Commands (`/anthro-plan` & `/anthro-revise`) 활용 (권장)

**설정 > Antigravity**의 **Google Antigravity 연동** 카드에서 전역 스킬을 설치하면 모든 Antigravity 워크스페이스에서 슬래시 명령어를 사용할 수 있습니다:

- **모두 설치** (`antigravity.btnInstallAll`)를 클릭하거나 각 명령어 옆의 **설치** (`antigravity.commandBtnInstall`)를 클릭합니다.

### 새로운 구현 계획 생성:
```text
/anthro-plan <구현하려는 과제 또는 기능 설명>
```
*저장소 컨텍스트를 수집하고 `anthro-bridge/plan`을 호출한 뒤, 파일 수정이나 빌드 명령 실행 없이 안전하게 멈추어 계획을 제시합니다.*

### 기존 구현 계획 수정 및 피드백 반영:
```text
/anthro-revise <반영할 피드백 또는 변경 사항>
```
*활성 컨텍스트 또는 `implementation_plan.md`에서 현재 구현 계획을 확인하고, 피드백과 함께 `anthro-bridge/plan`에 전달하여 영향을 받지 않은 부분을 보존하면서 계획을 업데이트합니다.*

> [!IMPORTANT]
> `/anthro-plan` 또는 `/anthro-revise`를 통해 실행 중일 때는 명령어 자체가 단일 planner 호출을 관리하므로 Workspace Rule로 인한 중복 planner 호출이 발생하지 않습니다.

---

## 7. Workspace Rule을 통한 계획 수립 자동화

프로젝트에 [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md)와 같은 Workspace Rule을 배치하면 복잡한 작업 시 외부 플래너 호출을 자동화할 수 있습니다:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
   Note: "Exactly once" means duplicate planner calls are prohibited once a successful usable result is obtained. If the tool call itself fails or returns an unusable response (e.g. transport or decoding error), exactly 1 recovery retry is permitted.
5. Use the returned DeepSeek plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

### 트리거 정책:
- **경미하거나 국소적인 작업 (Trivial / localized tasks)** (예: 오타 수정, 한 줄 수정, 사소한 문법 정리 등): 플래너를 호출하지 않습니다.
- **주요 작업 (Non-trivial tasks)** (아키텍처 변경, 여러 파일에 걸친 기능 구현, 복잡한 디버깅 등): Antigravity가 코드베이스를 조사하고 `anthro-bridge/plan`을 1회 호출하여 반환된 계획을 기반으로 구현을 진행합니다.

---

## 8. 일반적인 자동화 워크플로

```text
사용자: "다중 프로필을 지원하도록 기능 X를 리팩토링해줘."
    ↓
Antigravity가 코드를 조사하고 컨텍스트 요약
    ↓
Antigravity가 자동으로 anthro-bridge/plan 도구 호출 (정확히 1회)
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

- **독립적인 작동**: MCP 서버는 Anthro Bridge 3P Gateway와 완전히 독립적으로 작동합니다. MCP 도구를 사용하기 위해 3P Gateway를 실행(ON)해 둘 필요는 없습니다.
- **과금 분리**: `anthro-bridge/plan` 호출에는 선택한 제공업체의 API 비용이 발생합니다. 이후의 파일 수정 및 테스트는 Antigravity 자체의 구독 용량을 사용합니다.
- **실시간 반영**: Anthro Bridge GUI에서 플래너 제공업체나 모델 설정을 변경하면 다음 `plan()` 호출 시 즉시 적용됩니다.
��게 멈추고 사용자의 확인을 기다립니다.

---

## 7. Workspace Rule을 통한 계획 수립 자동화

[`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md)와 같은 Workspace Rule을 프로젝트에 배치하면 복잡한 작업 시 플래너 호출을 자동화할 수 있습니다:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the single planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
5. Use the returned plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
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
