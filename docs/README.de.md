[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Verwende Claude Code / Claude Desktop als Entwicklungsumgebung, leite Inferenz an Drittanbieter-LLM-APIs weiter und nutze externe Modelle als Planer und Reviewer für Google Antigravity.**

Anthro Bridge ist eine Windows-Begleitanwendung für KI-gestützte Softwareentwicklung. Es unterstützt zwei sich ergänzende Arbeitsabläufe:

1. **3P-Gateway für Claude Code / Claude Desktop** — Behalte Claudes Repository-Erkundung, Tool-Nutzung, Dateibearbeitung und Testausführung bei, während die Inferenz an Drittanbieter weitergeleitet wird.
2. **MCP-Planer & Reviewer für Google Antigravity** — Delegiere Implementierungsplanung und Nachimplementierungsüberprüfung über die MCP-Tools `anthro-bridge/plan` und `anthro-bridge/review` an externe Modelle.

---

## Zwei Hauptarbeitsabläufe

### 1. Claude Code / Claude Desktop mit 3P-Gateway

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P-Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **Trennung von Umgebung und Modell**: Behalte Claudes agentische Tooling-Infrastruktur, während die Inferenz an Drittanbieter weitergeleitet wird.
- **Dynamisches Multi-Profil-Routing**: Wechsle aktive Anbieter, OpenRouter-Profile und Modellrouten über die grafische Oberfläche.
- **Einrichtungsanleitung**: [Claude Desktop / Cowork 3P-Gateway-Einrichtung](THIRD_PARTY_INFERENCE.de.md)

### 2. Antigravity mit MCP-Planer & Reviewer

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Konfiguriertes externes Modell (Planer / Reviewer)
    ↓
Implementierungsplan / Review-Urteil
    ↓
Antigravity implementiert und testet
unter Nutzung abonnementbasierter Kapazität
```

- **Trennung von Planung und Ausführung**: Externe Modelle erstellen den übergeordneten Plan oder das Review-Urteil; die Antigravity-Abonnementkapazität führt tokenintensive Code-Änderungen durch.
- **Live-GUI-Konfiguration**: Das Wechseln des Planer- oder Reviewer-Anbieters, Modells oder Reasoning-Aufwands wird beim nächsten Aufruf sofort wirksam.
- **Einrichtungsanleitung**: [Google Antigravity + Anthro Bridge MCP-Einrichtung](ANTIGRAVITY_MCP.de.md)

**Globale Antigravity-Befehle:**

- **`/anthro-plan`** — Delegiere die Implementierungsplanung an das konfigurierte externe Modell.
- **`/anthro-revise`** — Überarbeite einen bestehenden Plan auf der Grundlage neuer Anforderungen oder Einschränkungen.
- **`/anthro-review`** — Überprüfe eine abgeschlossene Implementierung anhand des genehmigten Plans vor dem Commit, mit expliziten READY / NOT READY-Urteilen.

**Empfohlener Arbeitsablauf:**

```text
/anthro-plan → Implementierung & Tests → /anthro-review → Commit
```

---

## Unterstützte Anbieter

| Anbieter | Verbindung | Unterstützte Familien | Reasoning-Steuerung |
|---|---|---|---|
| **DeepSeek** | Direkte API | DeepSeek V4.1 Flash, V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | Direkte API | kimi-for-coding, kimi-for-coding-highspeed | Thinking-Modus |
| **MiniMax** | Direkte API | MiniMax M3, M2.7 | Modellspezifisch |
| **Kimi / Moonshot** | Direkte API | Kimi K2.x, Kimi K3 | Thinking / Reasoning-Aufwand |
| **MiMo / Xiaomi** | Direkte API | MiMo V2.6 Flash, Pro, Pro-UltraSpeed (V2.5 abwärtskompatibel) | Normal / Thinking |
| **OpenRouter** | Multi-Profil-Gateway | Siehe OpenRouter-Abschnitt unten | Modell- / Profilspezifisch |

### DeepSeek (Direkt)

Das integrierte Preset **Direct DeepSeek** leitet Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low weiter.

- `deepseek-v4.1-flash` — Aktuelles Flaggschiff-Reasoning-Modell ($0,27 / 1M Eingabe · $1,10 / 1M Ausgabe).
- `deepseek-v4-pro-0813` — Hochwertige Basislinie ohne erweitertes Reasoning ($0,27 / 1M Eingabe · $1,10 / 1M Ausgabe).

### Kimi Code (Direkt)

Dedizierte Coding-Spezialist-API (`KIMI_CODE_API_KEY`), getrennt von Moonshot Kimi:

- `kimi-for-coding` — Vollqualitäts-Coding-Modell.
- `kimi-for-coding-highspeed` — Variante mit niedrigerer Latenz.

### MiMo / Xiaomi (Direkt)

Integrierte **Direct MiMo** Preset-Routen:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- Standardmodell: `mimo-v2.6-flash`

Modelle: `mimo-v2.6-flash`, `mimo-v2.6-pro`, `mimo-v2.6-pro-ultraspeed` (wählbar). Alle drei unterstützen ein Kontextfenster von 1 Million Tokens und native multimodale Fähigkeiten (Text, Bild, Video).

**Normal / Thinking**: MiMo verwendet einen einfachen Normal/Thinking-Umschalter — keine Reasoning-Aufwandsstufen.

**V2.5-Abwärtskompatibilität**: Gespeicherte Routen `mimo-v2.5`, `mimo-v2.5-pro` und `mimo-v2.5-pro-ultraspeed` bleiben erhalten und funktionieren weiterhin. Unveränderte Legacy-Standardwerte werden beim Start automatisch auf V2.6 migriert.

### OpenRouter

Unterstützt mehrere benannte Profile. Vollständiger OpenAI-Modellkatalog (einzelnes Dropdown):

| Modell-ID | Anzeigename |
|---|---|
| `openai/gpt-6-astra` | GPT-6 Astra |
| `openai/gpt-6-astra-pro` | GPT-6 Astra Pro |
| `openai/gpt-astra-latest` | GPT Astra Latest |
| `openai/gpt-5.6-sol` | GPT-5.6 Sol |
| `openai/gpt-5.6-sol-pro` | GPT-5.6 Sol Pro |
| `openai/gpt-5.6-terra` | GPT-5.6 Terra |
| `openai/gpt-5.6-terra-pro` | GPT-5.6 Terra Pro |
| `openai/gpt-5.6-luna` | GPT-5.6 Luna |
| `openai/gpt-5.6-luna-pro` | GPT-5.6 Luna Pro |

**GPT-6 Astra**: 1,05M Kontext · Reasoning-Aufwand: `low / medium / high / xhigh / max`.  
**GPT-6 Astra Pro**: 1,05M Kontext · dauerhaftes Pro-Reasoning (`reasoning.mode = pro`), kein benutzerwählbarer Aufwand.  
**GPT Astra Latest**: Alias, der das neueste Modell der Astra-Familie verfolgt.

Integriertes Preset **OpenRouter: chatGPT**: Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium.

Ebenfalls verfügbar: **OpenRouter: Gemini** (Gemini 3.8 Flash · Reasoning-Aufwand `low / medium / high`), **OpenRouter: Poolside**, **OpenRouter: Tencent**, **OpenRouter: InclusionAI**, **OpenRouter: StepFun**.

---

## Modellpreise (ab v0.22.1)

| Modell | Eingabe | Ausgabe |
|---|---|---|
| DeepSeek V4.1 Flash | \$0,27 / 1M | \$1,10 / 1M |
| DeepSeek V4 Pro 0813 | \$0,27 / 1M | \$1,10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash (OpenRouter) | \$0,75 / 1M | \$3,75 / 1M |
| MiMo-V2.6-Flash | \$0,14 / 1M | \$0,28 / 1M |
| MiMo-V2.6-Pro | \$0,435 / 1M | \$0,87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4,35 / 1M | \$8,70 / 1M |

---

## Installation

Lade das neueste Windows-Installationsprogramm (`Anthro Bridge_x.x.x_x64-setup.exe`) von der [Releases](https://github.com/soheidon/anthro-bridge/releases)-Seite herunter und führe es aus.

Das Installationsprogramm unterstützt 8 Sprachen und bewahrt bestehende Benutzereinstellungen bei Upgrades.

---

## Schnellstart

### Arbeitsablauf 1: 3P-Gateway für Claude Code / Claude Desktop

1. Öffne Anthro Bridge unter **Einstellungen > API-Schlüssel** und konfiguriere einen API-Schlüssel für deinen gewünschten Anbieter.
2. Wähle deinen Anbieter oder dein OpenRouter-Profil im Dashboard aus.
3. Klicke auf **Gateway starten** (läuft auf `http://127.0.0.1:4000`).
4. Verbinde Claude Code oder Claude Desktop:
   - **Claude Code**: Klicke in den Einstellungen auf **Claude Code-Startbefehl kopieren** und füge ihn in PowerShell ein.
   - **Claude Desktop / Cowork**: Folge der [Claude Desktop 3P-Einrichtungsanleitung](THIRD_PARTY_INFERENCE.de.md).

### Arbeitsablauf 2: MCP-Planer & Reviewer für Google Antigravity

1. Konfiguriere in Anthro Bridge einen API-Schlüssel für dein gewähltes Planer-/Reviewer-Modell.
2. Wähle den **MCP**-Tab und konfiguriere dein Modell unter **Einstellungen > Antigravity > MCP-Planeinstellungen**.
3. Registriere `anthro-bridge.exe` mit `["--mcp-server"]` in Antigravitys MCP-Konfiguration (oder klicke in Anthro Bridge auf **Automatisch konfigurieren**).
4. Verwende `/anthro-plan` zum Erstellen von Plänen, `/anthro-revise` zum Aktualisieren von Plänen und `/anthro-review` zum Überprüfen von Implementierungen vor dem Commit.
5. Folge der vollständigen [Antigravity MCP-Einrichtungsanleitung](ANTIGRAVITY_MCP.de.md).

---

## API-Schlüssel

| Anbieter | Umgebungsvariable |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## Dokumentation

- [Claude Desktop / Cowork 3P-Gateway-Einrichtung](THIRD_PARTY_INFERENCE.de.md)
- [Google Antigravity + Anthro Bridge MCP-Einrichtung](ANTIGRAVITY_MCP.de.md)
- [Konfigurationsreferenz (`config.json`)](CONFIGURATION.md)
- [Anbieterdetails & Reasoning-Steuerung](PROVIDERS.md)
- [Entwicklungs- & Verifikationsanleitung](DEVELOPMENT.md)

---

## Fehlerbehebung

### Port 4000 ist bereits belegt
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Einstellungen werden nach einem Upgrade zurückgesetzt
Starte die Anwendung neu, damit Migrationen ausgeführt werden können. Die Konfiguration wird unter `%APPDATA%\Anthro Bridge\config.json` gespeichert.

### MCP-Planer-Aufrufe schlagen fehl
Stelle sicher, dass ein API-Schlüssel für den unter dem **MCP**-Tab ausgewählten Anbieter gesetzt ist oder in deinen Windows-Benutzerumgebungsvariablen exportiert wurde (z. B. `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`). Das 3P-Gateway muss für MCP nicht laufen.

---

## Lizenz

MIT-Lizenz. Siehe [LICENSE](../LICENSE).
