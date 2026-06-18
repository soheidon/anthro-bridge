[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## Überblick

Anthro Bridge ist ein Proxy- + GUI-Verwaltungstool, das Claude Desktop / Claude Code API-Anfragen über mehrere Anbieter-Antwortpunkte mit Anthropic-Kompatibilität weiterleitet.

Anthro Bridge liest das `model`-Feld jeder Anfrage und leitet diese automatisch an den richtigen Upstream-Anbieter weiter (modellbasiertes Routing). Nur das `model`-Feld wird überschrieben — Nachrichten, Thinking-Blöcke, tool_use, tool_result und Streaming-SSE werden unverändert durchgereicht.

Anthro Bridge ist kein Fork, keine GUI- oder Begleitapp für Moon Bridge; es ist ein unabhängiges Anthropic-kompatibles Gateway.

### Unterstützte Anbieter

| Anbieter-ID | Anzeigename | Upstream-Endpunkt | Standardmodell |
|-------------|-------------|-------------------|----------------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

Das GUI-Verwaltungstool (Tauri v2 + React 19 + TypeScript) bietet Start/Stopp-Steuerung, Konfigurationsbearbeitung, Protokollanzeige und API-Schlüsselverwaltung aus einem nativen Windows-Fenster.

### Warum dieses Gateway benötigt wird

Claude Desktop / Claude Code erwartet grundsätzlich das API-Format und die Modellnamen der Claude-Familie von Anthropic. Selbst wenn Anbieter wie DeepSeek, MiniMax, Kimi und MiMo Anthropic-kompatible APIs anbieten, kann Claude Desktop / Claude Code diese nicht immer direkt verwenden.

Insbesondere **akzeptiert die `inferenceModels[].name` von Claude Desktop nur offizielle Anthropic-Modellnamen**. Gateway-eigene Namen wie `claude-deepseek-v4` oder `kimi-k2.6` werden als `"not an Anthropic model"` abgelehnt.

Um diese Einschränkung zu umgehen, zeigt Anthro Bridge **offizielle Anthropic-Modellnamen (`claude-sonnet-4-6` / `claude-haiku-4-5`) als "Hüllen" für Claude Desktop, während das eigentliche LLM (DeepSeek / MiniMax / Kimi / MiMo) in der GUI ausgewählt wird**.

```
Claude Desktop Seite (immer fest)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Gateway intern (basierend auf GUI-Auswahl)
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

Dies ermöglicht es Ihnen, die Modellnamen-Validierung von Claude Desktop zu bestehen und gleichzeitig frei zwischen DeepSeek, MiniMax, Kimi und MiMo zu wechseln.

### Voraussetzungen

- **Windows 10/11** (Japanische Locale unterstützt)
- API-Schlüssel für Ihren gewählten Anbieter (DeepSeek / MiniMax / Kimi / MiMo — **einer reicht**, seit v0.5.0)

### Schnellstart

#### 1. Installation

Laden Sie das neueste Installationsprogramm von [Releases](https://github.com/soheidon/anthro-bridge/releases) herunter und führen Sie es aus.

Das Installationsprogramm zeigt beim Start eine Sprachauswahl an (wählen Sie aus English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español).

#### Aktualisieren

Führen Sie einfach das neue `setup.exe` aus — es erkennt und ersetzt automatisch die vorherige Version. Eine manuelle Deinstallation ist nicht erforderlich. Ihre Einstellungen (`%APPDATA%\Anthro Bridge\config.json`) bleiben bei Updates erhalten.

#### 2. API-Schlüssel festlegen

Einstellungen (⚙) -> **API-Schlüssel** Tab, geben Sie den API-Schlüssel Ihres Anbieters ein und klicken Sie auf **Speichern**.
Der Schlüssel wird als Windows-Benutzer-Umgebungsvariable gespeichert.

| Anbieter | Umgebungsvariable | Anmerkungen |
|----------|-------------------|-------------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. Anbieter auswählen

Auf dem Dashboard sind die Anbieter-Kacheln in einem 2×2-Raster angeordnet:

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

Klicken Sie auf eine Kachel, um den aktiven Anbieter unter **LLM-Anbieter wählen** auszuwählen.

#### 4. Gateway starten

Klicken Sie im Header auf **Gateway starten**. Das Proxy startet auf `http://127.0.0.1:4000`.

#### 5. Claude Desktop / Cowork on 3P konfigurieren

Detaillierte Schritt-für-Schritt-Anleitungen finden Sie unter [docs/THIRD_PARTY_INFERENCE.md](docs/THIRD_PARTY_INFERENCE.md).

### Endpunkte

| Methode | Pfad | Beschreibung |
|---------|------|--------------|
| GET | `/health` | Gesundheitscheck |
| GET | `/v1/models` | Öffentliche Modelliste |
| POST | `/v1/messages` | Messages API (stream + non-stream). Modellbasiertes Routing |
| POST | `/v1/messages/count_tokens` | Token-Zählung (nur unterstützte Anbieter) |

### Routing

Modellbasiertes Routing: Das `model`-Feld in jeder Anfrage bestimmt den Zielanbieter und das Upstream-Modell.

| Anthropic-Modell | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|------------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking an) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking aus) | `mimo-v2.5` |

#### MiMo Routing-Details

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`**: Thinking ist **standardmäßig aktiviert** (`thinking_mode: "thinking"`). Der `thinking_mode`-Schlüssel (nicht `thinking`) steuert MiMo's Thinking-Verhalten. Auf `"default"` für den Standardmodus setzen.
- **`claude-haiku-4-5` → `mimo-v2.5`**: Unterstützt Bild-Pass-Through (Bild-URL und Base64). Audio-/Video-Eingabe wird von Anthro Bridge bei MiMo nicht unterstützt.
- **`claude-sonnet-4-6`-Route unterstützt KEINE Bilder.** Wenn Bilder an diese Route gesendet werden, werden sie durch Text-Platzhalter ersetzt (`non_vision_image_policy: "replace"`).
- **Upstream-Endpunkt**: Anfragen werden an `https://api.xiaomimimo.com/anthropic/v1/messages` gesendet.

### Sprachen

8 Sprachen: English, 日本語,中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español.

Um eine neue Übersetzung hinzuzufügen, legen Sie eine Sprachdatei (z.B. `es.ts`) in `gui/src/i18n/lang/` ab und bauen Sie neu.
Details finden Sie unter [CONTRIBUTING](CONTRIBUTING.md).

### Konfiguration (config.json)

Anbieter-Einstellungen definieren Upstream-Modellnamen und Funktionsflags pro Modell. Normalerweise ist keine Bearbeitung erforderlich.
Fortgeschrittene Benutzer können über Einstellungen (⚙) -> **Gateway-Konfiguration** bearbeiten.

| Schlüssel | Beschreibung |
|-----------|--------------|
| `models.<model>.upstream_model` | Tatsächlicher Modellname, der an Upstream gesendet wird (erforderlich) |
| `models.<model>.thinking` | Bei `"disabled"` wird Thinking-Unterdrückung injiziert (optional). Für MiMo stattdessen `thinking_mode` verwenden |
| `models.<model>.thinking_mode` | MiMo-spezifisch: `"thinking"` (aktiviert) oder `"default"` (Standard). Nur vom MiMo-Anbieter verwendet |
| `models.<model>.supports_vision` | Bildunterstützung pro Modell (Fallback auf Anbieter-Standard) |
| `models.<model>.supports_video` | Videounterstützung pro Modell (Fallback auf Anbieter-Standard) |
| `models.<model>.visible` | Ob in `/v1/models` und Dashboard angezeigt (Standard `true`) |
| `non_vision_image_policy` | Bildverarbeitung für Nicht-Vision-Modelle: `replace` (Platzhalter) / `drop` / `reject` (Fehler) |

### Projektstruktur

```
anthro-bridge/
├── README.md
├── SPEC.md                    Spezifikation
├── docs/
│   ├── README.ja.md           Japanisch
│   ├── README.zh-CN.md        Chinesisch (Vereinfacht)
│   ├── README.de.md           Deutsch
│   ├── README.es.md           Spanisch
│   ├── SPEC.ja.md             Japanisch
│   ├── SPEC.zh-CN.md          Chinesisch (Vereinfacht)
│   ├── SPEC.de.md             Deutsch
│   ├── SPEC.es.md             Spanisch
│   ├── THIRD_PARTY_INFERENCE.md   Drittanbieter-Inferenz-Anleitung
│   ├── THIRD_PARTY_INFERENCE.ja.md
│   ├── THIRD_PARTY_INFERENCE.zh-CN.md
│   ├── THIRD_PARTY_INFERENCE.de.md
│   └── THIRD_PARTY_INFERENCE.es.md
├── LICENSE                    MIT-Lizenz
├── config.json                Anbieter-Konfiguration
├── .gitignore
└── gui/
    ├── src/                   React-Frontend (TypeScript)
    │   ├── components/        UI-Komponenten
    │   ├── hooks/             Benutzerdefinierte Hooks
    │   └── i18n/              Mehrsprachunterstützung
    │       └── lang/          Sprachdateien (en, ja, zh-CN, zh-TW, ko, fr, de, es)
    ├── src-tauri/             Tauri-Backend (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 Tauri-Befehle + Proxy-Lebenszyklus
    │   │   ├── main.rs        Einstiegspunkt
    │   │   └── proxy.rs       axum-Proxy-Server
    │   ├── resources/
    │   │   └── config.json    Gebündelte Konfiguration
    │   └── Cargo.toml
    └── package.json
```

### Entwicklung

```bash
cd gui
npm install
npm run tauri build    # Produktionsbau
npm run tauri dev      # Entwicklungsmodus (HMR)
```

Erfordert [Rust](https://rustup.rs/) stable Toolchain und Node.js 24+.

### Fehlerbehebung

#### Port 4000 belegt

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Bild/Video abgelehnt

DeepSeek unterstützt keine Bilder oder Videos. Bilder werden automatisch durch Platzhaltertext ersetzt (`non_vision_image_policy: "replace"`). Um Bilder nativ zu verwenden, wechseln Sie zu MiniMax, Kimi oder MiMo (`claude-haiku-4-5`-Route).

MiMo's `claude-sonnet-4-6`-Route unterstützt ebenfalls keine Bilder — verwenden Sie `claude-haiku-4-5` für Bildaufgaben. Video wird immer abgelehnt.

#### MiMo: Bestehende Benutzerkonfiguration wird nicht aktualisiert

Wenn Sie von einer Version vor v0.9.0 aktualisiert haben, hat Ihre gespeicherte Benutzerkonfiguration möglicherweise noch die alten Werte `"display_name": "MiMo"` oder `"thinking": "default"`. v0.9.0 migriert diese automatisch beim ersten Start, aber bei Problemen:

1. **App neu starten** — Die Automigration läuft beim Start.
2. **Konfiguration zurücksetzen**: Löschen Sie `%APPDATA%\Anthro Bridge\config.json` und starten Sie neu. Die gebündelte Konfiguration mit den korrekten MiMo-Einstellungen wird verwendet.
3. **Manuelle Prüfung**: Öffnen Sie `%APPDATA%\Anthro Bridge\config.json` und prüfen Sie, dass `providers.mimo` `"display_name": "MiMo / Xiaomi"`, `"api_key_env": "XIAOMI_API_KEY"` und `thinking_mode` (nicht `thinking`) in den Modelleinträgen hat.

### Manuelles Testen — MiMo / Xiaomi

#### Nur Text (claude-sonnet-4-6 → mimo-v2.5-pro)

1. `XIAOMI_API_KEY` in Einstellungen → API-Schlüssel Tab → Speichern festlegen.
2. **MiMo / Xiaomi** auf dem Dashboard auswählen.
3. Gateway starten.
4. Senden Sie eine Nachricht über Claude Desktop. Überprüfen Sie, dass die Antwort mit Thinking-Blöcken eintrifft.

#### Bildtest (claude-haiku-4-5 → mimo-v2.5)

1. **MiMo / Xiaomi** auf dem Dashboard auswählen.
2. Hängen Sie in Claude Desktop ein Bild an eine Nachricht an und senden Sie es.
3. Überprüfen Sie, dass das Bild empfangen und korrekt beschrieben wird.
4. Das Senden eines Bilds an `claude-sonnet-4-6` sollte zu einem Platzhaltertext-Ersatz führen.

#### Überprüfung

Prüfen Sie das Log-Panel in der GUI — Anfragen sollten das `model`-Feld nach `mimo-v2.5-pro` oder `mimo-v2.5` je nach Route umgeschrieben zeigen.

### Lizenz

MIT — siehe [LICENSE](LICENSE) für Details.
