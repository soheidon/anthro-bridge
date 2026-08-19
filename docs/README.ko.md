[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Claude Code Desktop을 코딩 하네스로 사용하고, 구현을 서드파티 API로 라우팅하며, 외부 모델을 Antigravity의 플래너로 활용하세요.**

Anthro Bridge는 AI 지원 소프트웨어 개발을 위한 Windows 컴패니언 애플리케이션으로, 두 가지 주요 워크플로를 중심으로 설계되었습니다.

1. **Claude Code / Claude Desktop + 서드파티 게이트웨이 (3P Gateway)**: Claude Code Desktop을 에이전트 코딩 하네스로 그대로 사용하면서, 로컬 Anthropic 호환 3P 게이트웨이를 통해 서드파티 LLM API(DeepSeek, MiMo, MiniMax, Kimi, OpenRouter)로 모델 요청을 라우팅합니다.
2. **Antigravity + MCP 플래너 (MCP Planner)**: Anthro Bridge MCP의 `plan` 도구(`anthro-bridge/plan`)를 통해 아키텍처 설계 및 구현 계획 수립을 외부 모델에 위임하고, 실제 파일 편집과 테스트는 Antigravity 구독에 포함된 모델 용량으로 수행합니다.

---

## 두 가지 주요 워크플로

### 1. Claude Code / Claude Desktop with 3P Gateway

Claude Code Desktop 및 Claude Desktop을 코딩 하네스로 사용하면서, Anthropic 클라이언트가 기본적으로 지원하지 않는 서드파티 LLM API로 모델 요청을 라우팅합니다.

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **하네스와 모델의 분리**: Claude의 저장소 탐색, 도구 사용, 파일 편집, 테스트 실행 기능을 유지하면서 서드파티 제공업체로 추론을 라우팅합니다.
- **동적 다중 프로필 라우팅**: GUI 대시보드에서 활성 제공업체 또는 OpenRouter 프로필을 자유롭게 전환하고 설정에서 Opus, Sonnet, Haiku 라우트를 커스터마이징합니다.
- **설정 가이드**: [Claude Desktop / Cowork 3P Gateway 설정 가이드](THIRD_PARTY_INFERENCE.ko.md)

### 2. Antigravity with MCP Planner

Anthro Bridge MCP `plan` 도구(`anthro-bridge/plan`)를 통해 구현 계획 수립을 외부 모델에 위임하고, 실제 코드 수정 및 터미널 명령어 실행은 Antigravity의 구독 모델 용량을 사용합니다.

```text
Antigravity
    ↓
저장소 탐색 (컨텍스트 수집)
    ↓
anthro-bridge / plan (MCP)
    ↓
Anthro Bridge MCP 서버
    ↓
설정된 외부 LLM
    ↓
구조화된 구현 계획 반환
    ↓
Antigravity가 구독 용량으로
파일 편집, 빌드, 테스트 수행
```

- **계획과 구현의 분리**: 외부 모델이 고수준 계획을 생성하고, Antigravity 구독 용량이 토큰 소모가 많은 코드 수정 및 테스트 루프를 실행합니다.
- **실시간 GUI 설정**: Anthro Bridge GUI에서 플래너 제공업체, 모델, 추론 강도를 변경하면 다음 `plan()` 호출 시 즉시 반영됩니다.
- **설정 가이드**: [Google Antigravity + Anthro Bridge MCP 설정 가이드](ANTIGRAVITY_MCP.ko.md)

---

## 지원 제공업체

| 제공업체 | 연결 유형 | 지원 모델 제품군 | 추론 제어 |
|---|---|---|---|
| **DeepSeek** | 직접 API | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | 직접 API | MiniMax M3, M2.7 | 모델별 지원 |
| **Kimi / Moonshot** | 직접 API | Kimi K2.x, Kimi K3 | Thinking / 추론 강도 |
| **MiMo / Xiaomi** | 직접 API | MiMo V2.5, V2.5 Pro | Thinking 모드 |
| **OpenRouter** | 다중 프로필 게이트웨이 | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini 등 | 모델별 / 프로필별 |

---

## 설치

[Releases](https://github.com/soheidon/anthro-bridge/releases) 페이지에서 최신 Windows 설치 프로그램(`Anthro Bridge_x.x.x_x64-setup.exe`)을 다운로드하여 실행하세요.

설치 프로그램은 8개 언어(영어, 일본어, 중국어 간체, 중국어 번체, 한국어, 프랑스어, 독일어, 스페인어)를 지원하며 업그레이드 시 기존 사용자 설정을 보존합니다.

---

## 빠른 시작

### 워크플로 1: Claude Code / Claude Desktop용 3P Gateway

1. Anthro Bridge **설정 > API Key**를 열고 사용할 제공업체의 API 키를 설정합니다.
2. 대시보드에서 제공업체 또는 OpenRouter 프로필을 선택합니다.
3. **Gateway 시작 (Start Gateway)**을 클릭합니다(`http://127.0.0.1:4000`에서 대기).
4. Claude Code 또는 Claude Desktop을 연결합니다:
   - **Claude Code**: 설정 화면에서 **Claude Code 시작 명령 복사**를 클릭하고 PowerShell에 붙여넣어 실행합니다.
   - **Claude Desktop / Cowork**: [Claude Desktop 3P 설정 가이드](THIRD_PARTY_INFERENCE.ko.md)를 따릅니다.

### 워크플로 2: Google Antigravity용 MCP Planner

1. Anthro Bridge에서 사용할 플래너 모델의 API 키를 설정합니다.
2. **MCP** 탭을 선택하고 **설정 > MCP Plan 상세 설정**에서 플래너 모델과 추론 설정을 구성합니다.
3. Antigravity의 MCP 설정에 `anthro-bridge-mcp-server.exe`를 등록합니다.
4. Antigravity에서 `anthro-bridge/plan`을 호출하거나 Workspace Rule로 자동화합니다.
5. 자세한 지침은 [Google Antigravity + Anthro Bridge MCP 설정 가이드](ANTIGRAVITY_MCP.ko.md)를 참조하세요.

---

## 문서

- [Claude Desktop / Cowork 3P Gateway 설정 가이드](THIRD_PARTY_INFERENCE.ko.md)
- [Google Antigravity + Anthro Bridge MCP 설정 가이드](ANTIGRAVITY_MCP.ko.md)
- [설정 참조 (`config.json`)](CONFIGURATION.md)
- [제공업체 세부 정보 및 모델 동작](PROVIDERS.md)
- [개발 및 검증 가이드](DEVELOPMENT.md)

---

## 문제 해결

### 4000번 포트가 이미 사용 중인 경우
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 업그레이드 후 설정이 초기화된 경우
애플리케이션을 다시 시작하여 마이그레이션을 실행하세요. 설정 파일은 `%APPDATA%\Anthro Bridge\config.json`에 저장됩니다.

### MCP Planner 호출이 실패하는 경우
Anthro Bridge의 **MCP** 탭에서 선택된 제공업체의 API 키가 설정되어 있거나 Windows 사용자 환경 변수(`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY` 등)에 설정되어 있는지 확인하세요. MCP를 사용할 때 3P Gateway를 실행할 필요는 없습니다.

---

## 라이선스

MIT License. 자세한 내용은 [LICENSE](../LICENSE)를 참조하세요.
