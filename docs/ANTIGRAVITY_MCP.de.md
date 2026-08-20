[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Zurück zur Anthro Bridge README](README.de.md)

# Anthro Bridge MCP mit Google Antigravity verwenden

Anthro Bridge erfordert keine separate ausführbare MCP-Serverdatei. Die installierte `anthro-bridge.exe` fungiert sowohl als Desktop-GUI-Anwendung als auch als MCP-Server. Antigravity startet den MCP-Modus durch Aufruf derselben Datei mit `--mcp-server`.

```text
Normaler Start
anthro-bridge.exe
→ Anthro Bridge Desktop-App / 3P Gateway

MCP-Start
anthro-bridge.exe --mcp-server
→ Headless stdio MCP-Server für Antigravity
```

Dies ermöglicht es Agentenumgebungen wie Google Antigravity, Architektur- und Implementierungsplanung an externe LLMs (z. B. DeepSeek V4, MiMo, Kimi, MiniMax oder OpenRouter-Modelle) über `anthro-bridge/plan` zu delegieren, während die eigentlichen tokenintensiven Code-Änderungen, Terminalbefehle, Builds und Tests mit dem im Antigravity-Abonnement enthaltenen Modellkontingent durchgeführt werden.

---

## 1. Funktionsweise dieses Workflows

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Konfiguriertes externes Planer-Modell
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
2. Provider-Authentifizierung in Anthro Bridge oder in den Systemumgebungsvariablen für den gewünschten Planer konfiguriert.
3. **Google Antigravity** installiert und aktiv.

---

## 3. MCP-Server in Antigravity konfigurieren

### Methode 1 — GUI-Konfiguration über Anthro Bridge (Empfohlen)

1. Öffnen Sie Anthro Bridge und wechseln Sie zu **Einstellungen** (Reiter `[Einstellungen]`) > linke Sub-Navigation **Antigravity**.
2. Überprüfen Sie die Karte **Google Antigravity Integration**:
   - **Ziel-Programmdatei**: Zeigt standardmäßig den Pfad der aktuell laufenden `anthro-bridge.exe` an. Um eine andere Binärdatei zu verwenden (z. B. portable oder eigene Builds), klicken Sie auf **Ändern** (`antigravity.btnChangeExe`) und wählen die Datei aus.
   - **Registrieren / Aktualisieren**: Klicken Sie auf **Antigravity-Konfiguration aktualisieren** (`antigravity.btnUpdate`), um den `anthro-bridge`-Eintrag in `%USERPROFILE%\.gemini\config\mcp_config.json` sicher einzutragen oder zu aktualisieren, während alle anderen MCP-Server erhalten bleiben.
   - **Entfernen**: Klicken Sie auf **Konfiguration entfernen** (`antigravity.btnRemove`), um den Server aus Antigravity zu deregistrieren.
   - **Ordner öffnen**: Klicken Sie auf **Einstellungsordner öffnen** (`antigravity.btnOpenFolder`), um das Verzeichnis im Windows Explorer zu öffnen.

---

### Methode 2 — Manuelle Konfiguration (Erweitert)

1. Klicken Sie in Anthro Bridge unter **Einstellungen > Antigravity** auf **Einstellungsordner öffnen**, um `%USERPROFILE%\.gemini\config\` im Windows Explorer zu öffnen.
2. Öffnen oder erstellen Sie `mcp_config.json` und fügen Sie `anthro-bridge` unter `mcpServers` hinzu:

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

Für Entwicklungs-Builds verweisen Sie direkt auf die Release-Ausführungsdatei:
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
> Sie müssen **keine** API-Schlüssel in Antigravitys `mcp_config.json` eintragen. Der MCP-Server nutzt die bestehende Authentifizierungsauflösung von Anthro Bridge (automatisches Lesen aus Windows-Umgebungsvariablen wie `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` oder den gespeicherten App-Einstellungen).

---

## 4. MCP-Verbindung überprüfen

Bestätigen Sie in der Ansicht **Installed MCP Servers** in Antigravity, dass `anthro-bridge` erkannt wurde:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Planer-Modelle in Anthro Bridge konfigurieren

Anthro Bridge trennt die Planerauswahl klar von der detaillierten Parameterverwaltung:

1. **Oberster Reiter `MCP` (`MCP for Antigravity`)**:
   - Zeigt verfügbare Anbieter (DeepSeek, OpenRouter, MiniMax, MiMo, Kimi) und Profile als Kacheln an.
   - Ein Klick auf eine Kachel wechselt das aktive Planer-Ziel sofort.
2. **`Einstellungen` > `Antigravity`**:
   - Karte **MCP Plan Detaileinstellungen**: Konfigurieren Sie Modell, Thinking-Modus und Reasoning Effort pro Anbieter/Profil.
   - Karte **Google Antigravity Integration**: Verwalten Sie die MCP-Serverregistrierung und die Antigravity Commands (Global Skills).

> [!NOTE]
> Der Anthro Bridge MCP-Server liest die aktuelle Konfiguration bei jedem `plan()`-Toolaufruf dynamisch ein. Sie müssen den MCP-Server oder Antigravity **nicht** neu starten, wenn Sie Einstellungen in der GUI ändern.

---

## 6. Antigravity Commands (`/anthro-plan` & `/anthro-revise`) verwenden (Empfohlen)

Über **Einstellungen > Antigravity > Google Antigravity Integration** können Sie Global Skills installieren, um Slash-Befehle in jedem Antigravity-Workspace zu nutzen:

- Klicken Sie auf **Alle installieren** (`antigravity.btnInstallAll`) oder auf **Installieren** (`antigravity.commandBtnInstall`) neben dem jeweiligen Befehl.

### Neuen Implementierungsplan erstellen:
```text
/anthro-plan <Beschreibung der Aufgabe oder des Features>
```
*Sammelt den Repository-Kontext, ruft `anthro-bridge/plan` auf und stoppt sicher nach der Planpräsentation, ohne Dateien zu ändern oder Builds auszuführen.*

### Bestehenden Plan überarbeiten / Feedback einarbeiten:
```text
/anthro-revise <Einzuarbeitendes Feedback oder geänderte Anforderungen>
```
*Identifiziert den aktuellen Plan (aktiver Kontext oder `implementation_plan.md`), übergibt Plan und Feedback an `anthro-bridge/plan` und aktualisiert den Plan unter Beibehaltung unveränderter Abschnitte.*

> [!IMPORTANT]
> Bei der Ausführung über `/anthro-plan` oder `/anthro-revise` verwaltet der Befehl den einzelnen Planeraufruf. Workspace Rules lösen keine zusätzlichen doppelten Aufrufe aus.

---

## 7. Planung mit Workspace Rules automatisieren

Platzieren Sie eine Workspace-Regel wie [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) in Ihrem Projekt, um den externen Planer bei komplexen Programmieraufgaben automatisch aufzurufen:

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

### Auslöserichtlinie:
- **Triviale / lokale Aufgaben (Trivial / localized tasks)** (z. B. Tippfehler korrigieren, einzeilige Anpassungen, kleine Syntaxkorrekturen): Der Planer wird nicht ausgelöst.
- **Nicht-triviale Aufgaben (Non-trivial tasks)** (Architekturänderungen, dateiübergreifende Features, komplexes Debugging): Antigravity analysiert den Kontext, ruft 1x `anthro-bridge/plan` auf und führt die Implementierung auf Basis des Plans aus.

---

## 8. Typischer automatisierter Workflow

```text
Benutzer: "Refaktoriere Feature X für Multi-Profil-Unterstützung."
    ↓
Antigravity untersucht Code und fasst Kontext zusammen
    ↓
Antigravity löst automatisch den anthro-bridge/plan-Aufruf aus (genau 1x)
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

- **Unabhängiger Betrieb**: Der MCP-Server arbeitet vollständig unabhängig vom Anthro Bridge 3P Gateway. Das 3P Gateway muss nicht aktiv (eingeschaltet) sein, um den MCP-Server zu nutzen.
- **Getrennte Abrechnung**: Aufrufe von `anthro-bridge/plan` verursachen API-Kosten beim jeweiligen Drittanbieter. Nachfolgende Codeänderungen und Tests nutzen Ihr Antigravity-Abonnement.
- **Sofortige Wirksamkeit**: Änderungen von Planer-Anbietern oder Modellparametern in der Anthro Bridge GUI gelten unmittelbar für den nächsten `plan()`-Aufruf.
