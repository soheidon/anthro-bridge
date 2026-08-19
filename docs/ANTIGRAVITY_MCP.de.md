[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Zurück zur Anthro Bridge README](README.de.md)

# Anthro Bridge MCP mit Google Antigravity verwenden

Anthro Bridge enthält einen integrierten Model Context Protocol (MCP)-Server, der ein spezialisiertes `plan`-Tool (`anthro-bridge/plan`) bereitstellt. Dies ermöglicht es Agentenumgebungen wie Google Antigravity, Architektur- und Implementierungsplanung an externe LLMs (z. B. DeepSeek V4, MiMo, Kimi, MiniMax oder OpenRouter-Modelle) zu delegieren, während die eigentlichen tokenintensiven Code-Änderungen, Terminalbefehle, Builds und Tests mit dem im Antigravity-Abonnement enthaltenen Modellkontingent durchgeführt werden.

---

## 1. Funktionsweise dieses Workflows

```text
Antigravity
    ↓
Repository-Erkundung (Dateien prüfen und Kontext sammeln)
    ↓
anthro-bridge / plan (MCP-Aufruf mit Aufgabe, Kontext, Einschränkungen)
    ↓
Anthro Bridge MCP Server
    ↓
Externes Planer-Modell (in der Anthro Bridge GUI konfiguriert)
    ↓
Strukturierter Implementierungsplan wird zurückgegeben
    ↓
Antigravity führt Änderungen,
Builds und Tests über das Abonnement aus
```

- **Externe API**: Ausschließlich für die Generierung des Implementierungsplans basierend auf dem Repository-Kontext zuständig (wird vom jeweiligen Anbieter nutzungsbasiert abgerechnet).
- **Antigravity-Abonnement**: Übernimmt die aufwendigen Lese-/Schreibvorgänge, Code-Änderungen, Tool-Ausführungen und Testschleifen.
- **Aufgabenteilung**: Profitieren Sie von der überlegenen Planungsleistung externer Modelle, ohne API-Tokens für Routineaufgaben zu verschwenden.

---

## 2. Voraussetzungen

1. **Anthro Bridge** unter Windows installiert.
2. **`anthro-bridge-mcp-server.exe`** gebaut oder im Installationsverzeichnis vorhanden (z. B. `mcp-server/target/release/anthro-bridge-mcp-server.exe`).
3. Ein **API-Schlüssel** für den gewünschten Planer-Anbieter konfiguriert.
4. **Google Antigravity** installiert und aktiv.

---

## 3. MCP-Server in Antigravity konfigurieren

1. Öffnen Sie Google Antigravity.
2. Navigieren Sie zu:
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. Fügen Sie die `anthro-bridge`-Serverkonfiguration zu Ihrem `mcpServers`-Objekt hinzu:

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
> Sie müssen API-Schlüssel nicht im Klartext in der MCP-Konfiguration hinterlegen. Der MCP-Server liest automatisch Ihre Windows-Benutzerumgebungsvariablen (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` usw.) oder die in Anthro Bridge gespeicherte Konfiguration aus.

---

## 4. MCP-Verbindung überprüfen

Bestätigen Sie in der Ansicht **Installed MCP Servers** in Antigravity, dass `anthro-bridge` erkannt wurde:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Planer-Modell in Anthro Bridge konfigurieren

1. Öffnen Sie die Desktop-App **Anthro Bridge**.
2. Wählen Sie oben den Reiter **MCP** aus.
3. Wählen Sie den gewünschten Planer-**Anbieter (Provider)** oder das **Profil (Profile)** (z. B. DeepSeek, MiMo, OpenRouter).
4. Öffnen Sie **Einstellungen** (oder MCP Plan-Detaileinstellungen) und konfigurieren Sie:
   - **Modell (Model)**
   - **Thinking-Modus**
   - **Reasoning-Aufwand (Reasoning Effort)**
5. Speichern Sie die Einstellungen.

> [!NOTE]
> Der Anthro Bridge MCP-Server liest die aktuelle Konfiguration bei jedem `plan()`-Toolaufruf dynamisch ein. Sie müssen den MCP-Server oder Antigravity **nicht** neu starten, wenn Sie Einstellungen in der GUI ändern.

---

## 6. Das plan-Tool manuell aufrufen

Sie können Antigravity im Chat direkt anweisen, den Planer aufzurufen:

```text
Untersuche dieses Projekt und verwende dann das anthro-bridge/plan MCP-Tool, um einen Implementierungsplan zu erstellen. Führe noch keine Änderungen durch.
```

Antigravity prüft die relevanten Dateien, fasst den Kontext zusammen, ruft `anthro-bridge/plan` auf und präsentiert Ihnen den resultierenden Plan zur Überprüfung.

---

## 7. Planung mit einer Antigravity Workspace-Regel automatisieren

Erstellen Sie eine Workspace-Regeldatei unter [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md), um den Planer bei komplexen Codierungsaufgaben automatisch zu aktivieren:

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

## 8. Typischer automatisierter Workflow

```text
Benutzer: "Refaktoriere Feature X für Multi-Profil-Unterstützung."
    ↓
Antigravity untersucht Code und fasst Kontext zusammen
    ↓
Antigravity löst automatisch den anthro-bridge/plan-Aufruf aus
    ↓
Anthro Bridge sendet Prompt an das externe Modell
    ↓
Antigravity empfängt strukturierten Implementierungsplan
    ↓
Benutzer prüft und genehmigt den Plan
    ↓
Antigravity führt Codeänderungen durch und führt Tests aus
```

---

## 9. Wichtige Hinweise

- **Unabhängiger Betrieb**: Der MCP-Server arbeitet unabhängig vom Anthro Bridge 3P Gateway. Das 3P Gateway muss für MCP-Aufrufe nicht laufen.
- **Getrennte Abrechnung**: Aufrufe von `anthro-bridge/plan` verursachen API-Kosten beim jeweiligen Drittanbieter. Nachfolgende Codeänderungen und Tests nutzen Ihr Antigravity-Abonnement.
- **Sofortige Wirksamkeit**: Änderungen in der Anthro Bridge GUI gelten unmittelbar für den nächsten `plan()`-Aufruf.
