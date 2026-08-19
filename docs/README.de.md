[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Nutzen Sie Claude Code Desktop als Coding-Harness, leiten Sie Implementierungen an Drittanbieter-APIs weiter und verwenden Sie externe Modelle als Planer für Antigravity.**

Anthro Bridge ist eine Windows-Begleitanwendung für die KI-gestützte Softwareentwicklung, die um zwei Haupt-Workflows herum aufgebaut ist:

1. **Claude Code / Claude Desktop + 3P Gateway**: Behalten Sie Claude Code Desktop als Agenten-Coding-Harness bei, während Modellanfragen über ein lokales Anthropic-kompatibles 3P-Gateway an Drittanbieter-LLM-APIs (DeepSeek, MiMo, MiniMax, Kimi und OpenRouter) weitergeleitet werden.
2. **Antigravity + MCP Planner**: Delegieren Sie die Architektur- und Implementierungsplanung über das Anthro Bridge MCP `plan`-Tool (`anthro-bridge/plan`) an externe Modelle, während Code-Änderungen und Tests mit dem in Ihrem Antigravity-Abonnement enthaltenen Modellkontingent durchgeführt werden.

---

## Zwei Haupt-Workflows

### 1. Claude Code / Claude Desktop mit 3P Gateway

Nutzen Sie Claude Code Desktop und Claude Desktop weiterhin als Coding-Harness, während Anfragen an Drittanbieter-LLM-APIs weitergeleitet werden, die von Anthropic-Clients nativ nicht unterstützt werden.

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **Trennung von Harness & Modell**: Behalten Sie Claudes Repository-Erkundung, Tool-Nutzung, Datei-Bearbeitung und Test-Ausführung bei, während die Inferenz an Drittanbieter geleitet wird.
- **Dynamisches Multi-Profil-Routing**: Wechseln Sie aktive Anbieter oder OpenRouter-Profile direkt über das GUI-Dashboard und passen Sie die Routen Opus, Sonnet und Haiku in den Einstellungen an.
- **Setup-Anleitung**: [Claude Desktop / Cowork 3P Gateway Setup-Anleitung](THIRD_PARTY_INFERENCE.de.md)

### 2. Antigravity mit MCP Planner

Delegieren Sie Implementierungsplanung und Architekturdesign an externe Modelle über das Anthro Bridge MCP `plan`-Tool (`anthro-bridge/plan`), während Datei-Änderungen und Terminalbefehle über das Antigravity-Abonnementkontingent ausgeführt werden.

```text
Antigravity
    ↓
Repository-Erkundung (Kontext erfassen)
    ↓
anthro-bridge / plan (MCP)
    ↓
Anthro Bridge MCP Server
    ↓
Konfiguriertes externes LLM
    ↓
Strukturierter Implementierungsplan
    ↓
Antigravity führt Änderungen,
Builds und Tests über das Abo aus
```

- **Aufteilung von Planung & Ausführung**: Externe Modelle erstellen den übergeordneten Plan; das Antigravity-Abonnement übernimmt die tokenintensiven Codeänderungs- und Testschleifen.
- **Live-GUI-Konfiguration**: Änderungen an Planer-Anbieter, Modell oder Reasoning-Intensität in Anthro Bridge werden beim nächsten `plan()`-Aufruf sofort wirksam, ohne Antigravity neu zu starten.
- **Setup-Anleitung**: [Google Antigravity + Anthro Bridge MCP Setup-Anleitung](ANTIGRAVITY_MCP.de.md)

---

## Unterstützte Anbieter

| Anbieter | Verbindungstyp | Unterstützte Modellfamilien | Reasoning-Steuerung |
|---|---|---|---|
| **DeepSeek** | Direkte API | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | Direkte API | MiniMax M3, M2.7 | Modellspezifisch |
| **Kimi / Moonshot** | Direkte API | Kimi K2.x, Kimi K3 | Thinking / Reasoning-Aufwand |
| **MiMo / Xiaomi** | Direkte API | MiMo V2.5, V2.5 Pro | Thinking-Modus |
| **OpenRouter** | Multi-Profil-Gateway | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini usw. | Modellspezifisch / Profilspezifisch |

---

## Installation

Laden Sie das neueste Windows-Installationsprogramm (`Anthro Bridge_x.x.x_x64-setup.exe`) von der [Releases](https://github.com/soheidon/anthro-bridge/releases)-Seite herunter und führen Sie es aus.

Das Installationsprogramm unterstützt 8 Sprachen (Englisch, Japanisch, vereinfachtes Chinesisch, traditionelles Chinesisch, Koreanisch, Französisch, Deutsch, Spanisch) und behält bestehende Benutzereinstellungen bei Upgrades bei.

---

## Schnellstart

### Workflow 1: 3P Gateway für Claude Code / Claude Desktop

1. Öffnen Sie in Anthro Bridge **Einstellungen > API-Schlüssel** und konfigurieren Sie einen Schlüssel für den gewünschten Anbieter.
2. Wählen Sie Ihren Anbieter oder Ihr OpenRouter-Profil im Dashboard aus.
3. Klicken Sie auf **Gateway starten (Start Gateway)** (lauscht auf `http://127.0.0.1:4000`).
4. Verbinden Sie Claude Code oder Claude Desktop:
   - **Claude Code**: Klicken Sie in den Einstellungen auf **Claude Code Startbefehl kopieren** und fügen Sie ihn in PowerShell ein.
   - **Claude Desktop / Cowork**: Folgen Sie der [Claude Desktop 3P Setup-Anleitung](THIRD_PARTY_INFERENCE.de.md).

### Workflow 2: MCP Planner für Google Antigravity

1. Konfigurieren Sie einen API-Schlüssel für Ihr gewähltes Planer-Modell in Anthro Bridge.
2. Wählen Sie den Reiter **MCP** in Anthro Bridge und konfigurieren Sie Ihr Modell unter **Einstellungen > MCP Plan-Detaileinstellungen**.
3. Registrieren Sie `anthro-bridge-mcp-server.exe` in der MCP-Konfiguration von Antigravity.
4. Rufen Sie `anthro-bridge/plan` in Antigravity auf (oder automatisieren Sie es mit einer Workspace-Regel).
5. Folgen Sie der vollständigen [Antigravity MCP Setup-Anleitung](ANTIGRAVITY_MCP.de.md).

---

## Dokumentation

- [Claude Desktop / Cowork 3P Gateway Setup-Anleitung](THIRD_PARTY_INFERENCE.de.md)
- [Google Antigravity + Anthro Bridge MCP Setup-Anleitung](ANTIGRAVITY_MCP.de.md)
- [Konfigurationsreferenz (`config.json`)](CONFIGURATION.md)
- [Anbieter-Details & Modellverhalten](PROVIDERS.md)
- [Entwicklungs- & Verifizierungsleitfaden](DEVELOPMENT.md)

---

## Fehlerbehebung

### Port 4000 ist bereits belegt
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Einstellungen werden nach Upgrade zurückgesetzt
Starten Sie die Anwendung neu, damit Migrationen ausgeführt werden können. Die Konfiguration wird unter `%APPDATA%\Anthro Bridge\config.json` gespeichert.

### MCP Planner-Aufrufe schlagen fehl
Stellen Sie sicher, dass ein API-Schlüssel für den unter dem Reiter **MCP** ausgewählten Anbieter hinterlegt ist oder in Ihren Windows-Benutzerumgebungsvariablen (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY` usw.) exportiert wurde. Das 3P Gateway muss für MCP nicht laufen.

---

## Lizenz

MIT-Lizenz. Siehe [LICENSE](../LICENSE).
