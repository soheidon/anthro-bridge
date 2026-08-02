[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## Überblick

Ein schlankes Proxy- + GUI-Verwaltungstool, das Claude Desktop / Claude Code API-Anfragen über die Anthropic-kompatiblen Endpunkte mehrerer Anbieter weiterleitet.

### Architektur

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Eingebettet in Tauri-App (axum 0.7 + reqwest)
       |
       | Leitet per Modellfeld weiter -> löst korrekten Upstream-Anbieter auf
       | Überschreibt nur das Modell auf den Upstream-Namen
       | Injiziert Thinking-deaktiviert für Nicht-Thinking-Varianten
       | Medienunterstützungsprüfung pro Modell
       v
Anthropic-kompatible Anbieter-APIs
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### Designprinzipien

- **Shell-Modell + Anbieterauswahl**: Claude Desktop sieht immer `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`. Das eigentliche LLM wird in der GUI ausgewählt (DeepSeek / MiniMax / Kimi / MiMo / OpenRouter). Das Modellmapping des aktiven Anbieters wird für das Routing verwendet.
- **OpenRouter-Unterstützung**: Leitet an OpenRouters Anthropic-kompatiblen Endpunkt mit Poolside-Laguna-S/XS-Standards weiter. Dedizierte Thinking-Modus-Steuerungen (Max/On/Off) werden zur Anfragezeit in OpenRouters `reasoning`-Format übersetzt.
- **Nur der aktive Anbieter benötigt einen API-Schlüssel**: Seit v0.5.0 werden beim Start nur Anbieter überprüft, auf die die Routing-Tabelle verweist. Schlüssel nicht aktiver Anbieter sind nicht erforderlich.
- **Schlankes Proxy**: Außer dem `model`-Feld wird nichts geändert. SSE wird Byte für Byte weitergeleitet.
- **Verlustfreie Weiterleitung**: Nachrichten-Bodies, Tool-Aufrufe und Thinking-Blöcke werden unverändert durchgereicht.
- **Windows-natives GUI**: Tauri v2 + React 19 + TypeScript. Rust-Backend, Vite + React 19-Frontend.
- **Keine externen Abhängigkeiten**: Proxy seit v0.3.0 in die Tauri-Binary eingebettet. Python nicht erforderlich.
- **Mehrsprachig**: 8 Sprachen (en, ja, zh-CN, zh-TW, ko, fr, de, es). Neue Sprachen durch Ablegen von Dateien in `lang/` hinzufügen. Sprachauswahl beim ersten Start.
- **Reasoning-Aufwand**: DeepSeek V4 Pro unterstützt in Thinking-Modus den Reasoning-Aufwand High / Max; V4 Flash unterstützt Low / High / Max. Im Normalmodus ist der Reasoning-Aufwand deaktiviert. Ein für eine V4-Pro-Route gespeicherter veralteter `low`/`medium`-Aufwand wird beim Start zu `high` migriert.
- **Fähigkeitserkennung**: Live-Fähigkeitsflags (supports_image_url, supports_image_base64, supports_video_url, supports_video_base64) werden von der OpenRouter-API abgerufen und in config.json gespeichert.
- **Peak-/Valley-Preisbewusstsein**: Die Spitzenzeiträume von DeepSeek und OpenRouter werden in der lokalen Zeitzone angezeigt.
- **MiniMax-M3-Thinking-Umschalter**: MiniMax-M3 unterstützt Thinking AN/AUS über die Anthropic-kompatible API (`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`). M2.x-Modelle bleiben nur-Thinking. Eine Startmigration wandelt für bestehende Benutzer das veraltete `thinking_only` → `thinking` um.
- **Normalisierung der Antwortmodell-Identität**: Schreibt Upstream-Modellnamen in API-Antworten (sowohl SSE-Streaming als auch Nicht-Streaming) zurück auf die offiziellen Anthropic-Modellnamen. Gesteuert über `normalize_response_model_identity` in config.json und ein Laufzeit-`AtomicBool`. Unabhängiger Speicherbefehl (`update_normalize_model_identity`), um Kreuzkontamination mit Server-Konfigurationsspeicherungen zu vermeiden.
- **Strukturierte Kommunikationsprotokollierung**: `tracing` + `tracing-appender` schreibt strukturierte Logs in `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`. Jede Anfrage erhält eine Korrelations-ID aus einem `AtomicU64`-Zähler. Log-Einträge enthalten Anfragemodell, Gateway-Modell, Upstream-Modell, Normalisierungsergebnis und Überspringungsgründe. Es werden keine sensiblen Daten (Prompts, Bodies, API-Schlüssel) protokolliert.
- **PEAK-Abzeichen**: Ein farbcodiertes rosafarbenes Abzeichen im Dashboard für Modelle mit Spitzenpreis.
- **UTC-Offset-Anzeige**: Der Zeitzonen-Wähler zeigt neben jeder Option dynamische UTC-Offsets (z. B. UTC+09:00).
- **Erkennung des Token-Limit-Fehlers bei Laguna S/XS 2.1**: Erkennt reine Reasoning-Antworten mit `stop_reason: "max_tokens"` sowohl in SSE-Streams als auch in Nicht-Stream-Antworten. Protokolliert eine Warnung, wenn das Token-Limit pro Runde erreicht wird, ohne verwertbaren Text oder Tool-Aufrufe zu erzeugen. Über OpenRouter für alle Poolside-Laguna-Modelle verfügbar.
- **Poolside-thinking:disabled-Durchleitung**: Übersetzt das vom Client gesendete `thinking: { type: "disabled" }` für Poolside-Modelle in OpenRouters `reasoning: { enabled: false }`-Format und stellt so sicher, dass deaktiviertes Thinking auch ohne gespeicherte Konfigurationseinstellung korrekt weitergeleitet wird.
- **Migration des Laguna-Opus-Standards**: Eine einmalige idempotente Migration ändert für OpenRouter-Benutzer von `poolside/laguna-s-2.1` den Standard von `claude-opus-5` von Thinking-an auf Normalmodus. Die Vorlage für Neuinstallationen spiegelt den aktualisierten Standard wider.
- **OpenRouter-Mehrfachprofile**: Mehrere OpenRouter-Profile pro Benutzer, jeweils mit eigenem API-Schlüssel und eigener Modellkonfiguration. Profil-CRUD über Tauri-Befehle. Wechsel des aktiven Profils über Dashboard oder Einstellungen. Profile können per Drag-and-Drop neu angeordnet, ausgeblendet und in der konfigurierten Reihenfolge gespeichert werden.
- **OpenRouter-Dashboard-Karten**: Das Dashboard erstellt eine Karte pro sichtbarem OpenRouter-Profil, mit einer Fallback-Karte, wenn keine Profile vorhanden sind. Modellzusammenfassungen blenden den Anbieter-Namespace vor dem ersten `/` nur für die OpenRouter-Anzeige aus; die vollständigen Upstream-IDs bleiben für das Routing unverändert.
- **OpenRouter-Modellregister**: Ein lokales eingebautes Register bekannter OpenRouter-Modelle (`model_capabilities.rs`, `builtinOpenRouter.ts`) mit vorkonfigurierten Fähigkeiten (Vision, Video, Thinking-Richtlinie, Reasoning-Aufwand), Anbieter-Gruppierung und Preisdaten. Wird für die Modellklassifizierung ohne Live-API-Aufrufe verwendet.
- **OpenRouter-Preisdetails**: Die eingebauten Preise unterstützen aktuelle und überarbeitete Standardwerte für Prompt-, Ausgabe- und Cache-Eingabesätze, einschließlich der GPT-5.6-Varianten Sol, Terra, Luna und Pro. Die GUI zeigt Aktions- und Standardpreise zusammen an, wenn beide verfügbar sind.
- **GPT-5.6-Modellunterstützung**: OpenRouter-Profile können die Modellvarianten Sol, Terra und Luna mit fähigkeitsbewusster Thinking-Steuerung und Preisnotizen für Langkontextsätze verwenden, wo zutreffend. Das eingebaute OpenAI-GPT-5.6-Balanced-Profil routet bei Neuinstallationen Opus 5 → GPT-5.6 Sol, Sonnet 5 → GPT-5.6 Terra und Haiku 4.5 → GPT-5.6 Luna mit Reasoning-Aufwand Thinking High auf allen drei Routen; vorhandenes gespeichertes Routing wird nicht automatisch geändert.
- **Dashboard-gesteuerte Fenstergrößenberechnung**: Beim Start und bei Änderungen der Zeilenanzahl wird die Fensterhöhe aus den sichtbaren Dashboard-Karten in einem Drei-Spalten-Raster berechnet. Die Berechnung berücksichtigt Kartenhöhe, Rasterabstände, native Mindestgröße, Monitor-Arbeitsbereich, DPI-Skalierung und Fensterdekorationen, während die manuelle Größenänderung bei unveränderter Zeilenanzahl erhalten bleibt.
- **Lokalisierter NSIS-Installer**: Der Windows-Installer bietet die Sprachauswahl Englisch, Japanisch, Chinesisch (Vereinfacht), Chinesisch (Traditionell), Koreanisch, Französisch, Deutsch und Spanisch und bündelt das Anthro-Bridge-Anwendungssymbol.
- **Regressionsabdeckung**: Die Vitest-Abdeckung umfasst OpenRouter-Profilreihenfolge und Speicher-Races, Produktionspreisdaten, die Semantik der Dashboard-Kartenanzahl und monitorbewusste Fenstergrößenberechnung.
- **Neue Anbieter über OpenRouter**: InclusionAI und StepFun wurden als OpenRouter-Modellanbieter mit dedizierten Fähigkeitsflags, Thinking-Modus-Steuerung und Anbieter-Gruppierung hinzugefügt.
- **Tencent-Hy3-Thinking-Modi**: Unterstützung für niedrigen/hohen Reasoning-Aufwand für Tencents Hunyuan-Modell. Die Thinking-Modus-Übersetzung in proxy.rs bildet `thinking_mode` auf OpenRouters `reasoning`-Format ab. Die UI zeigt Low/High als Dropdown-Optionen an.
- **Kimi-K3-Korrekturen**: Hartcodierte `forced_reasoning_effort`-Einträge aus den Fähigkeitsdefinitionen entfernt. Die feste Anzeige „Max" wurde durch einen konfigurierbaren Dropdown-Wähler ersetzt. Standardwerte aus der gespeicherten Konfiguration, mit Rückfall auf „max".
- **Serialisierung von Konfigurationsschreibvorgängen**: Alle Tauri-Befehle, die Konfigurationen schreiben, werden über `execute_serialized_config_mutation` mit einem `Mutex`-Guard serialisiert. Die `ConfigState`-Struktur stellt mit Validierung `applied_config`-, `in_flight_config`- und `pending_ops`-Verfolgung bereit. Verhindert Race Conditions, wenn mehrere Einstellungsänderungen gleichzeitig gespeichert werden.
- **OpenRouter-UI-Race-Korrekturen**: (1) Der Latest-Callback-Ref `syncUiFromSavedRouteRef` verhindert, dass ein veralteter Closure die UI der neuen Route überschreibt. (2) Der Guard `rollbackRouteId` verhindert das Routen-übergreifende Rollback in Phase 2. (3) Der Hook `useRouteSaveGeneration` stellt für alle Handler `begin()`/`isCurrent()`-Generations-Guards bereit. (4) Save-Queue-Hook (`useOpenRouterSaveQueue`) mit Drain-Loop, Supersede-Erkennung und Neustart der OR-Aggregation.
- **Trennung der Dev-/Stable-App-Identität**: Die `AppChannel`-Enumeration (`Stable`/`Dev`) in `paths.rs` wählt getrennte Bezeichner (`com.soheidon.anthro-bridge` vs. `.dev`), Konfigurationsverzeichnisse (`Anthro Bridge` vs. `Anthro Bridge Dev`) und Cache-Pfade. Der Dev-Kanal verwendet `tauri.dev.conf.json`. NPM-Skripte: `npm run dev` (dev), `npm run dev:stable` (stable).
- **Einbettung der Konfigurationsvorlage**: `include_str!()` bettet `config_template.rs` zur Kompilierzeit ein und entfernt damit die Laufzeitabhängigkeit vom gebündelten `config.json`. `merge_bundled_providers` gibt ein `Result` mit typisierter Fehlerbehandlung zurück.
- **Frontend-Regressionstests**: 7 Vitest-Regressionstests für OpenRouter-Speicher-Race-Bedingungen unter Verwendung von `QueueHarness` und `GenerationHandlerHarness`. Die Tests decken ab: Latest-Callback-Ref, Routen-übergreifenden Rollback-Guard, Identitätserfassung, Refresh-Wiederholung (Fehler- und Erfolgspfade), In-Flight-Supersede und Generations-Guard.
- **Claude-Code-Kontextverwaltung**: Modellbewusste Auto-Kompaktierung für Claude Code. `resolve_effective_auto_compact` löst jede Standardroute (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) in ihr Upstream-Modell auf, schlägt die Kontextkapazität jedes Modells im statischen Register `model_context_windows.json` nach und verwendet im Auto-Modus die kleinste bekannte Kapazität als sicheres Kontextfenster. Die Kontextsteuerung greift nur, wenn alle drei Kapazitäten bekannt sind (andernfalls ist der Status Incomplete). Ein Umschalter in der Kopfzeile schaltet die Kontextverwaltung ein/aus; erweiterte Modi und Schwellenwerte werden in `config.json` unter `claude_code.auto_compact` festgelegt. Modi: `auto`, `manual` (`window_tokens`), `claude_default`.
- **Erzeugung des Claude-Code-Startbefehls**: `build_claude_code_launch_command` erzeugt einen vollständigen PowerShell-Befehl, der die Gateway-Verbindungsvariablen (`ANTHROPIC_BASE_URL`, das auf das lokale Gateway verweist, und `ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`) mit den Claude-Code-Kontextsteuerungsvariablen (`CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`) kombiniert. Wenn die Kontextverwaltung deaktiviert oder unvollständig ist oder auf den Claude-Standard gesetzt wurde, entfernt der Befehl veraltete Kontextvariablen mit `Remove-Item Env:... -ErrorAction SilentlyContinue`, damit zuvor gesetzte Sitzungswerte nicht in einen neuen Start übertragen werden. Die Schaltfläche „Claude Code Startbefehl kopieren" im Claude-Einstellungsbereich kopiert den Befehl in die Zwischenablage. Anthro Bridge erzeugt und kopiert den Befehl nur — es führt ihn nie aus.
- **Gemeinsames Modul für Modell-Routing**: `model_routing.rs` extrahiert die Auflösung von Route zu Upstream in reine Funktionen, die von `proxy.rs` und dem Kontextauflöser gemeinsam genutzt werden, und stellt so sicher, dass die Kontextfenster dieselben Upstream-Modelle auflösen, an die das Proxy tatsächlich weiterleitet.
- **Kontextkapazitäts-Register**: `model_context_windows.json` ist ein statisches Register bekannter Kontextkapazitäten, das die eingebauten Direktanbieter-Modelle (DeepSeek, MiniMax, Kimi, MiMo) und die eingebauten OpenRouter-Modelle (Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6) abdeckt. Unbekannte benutzerdefinierte OpenRouter-Modelle bleiben gültige Routenziele, melden die Kontextverwaltung jedoch als Incomplete, bis Metadaten hinzugefügt oder der manuelle Modus konfiguriert wird.

### GUI-Verwaltungstool

Tauri v2 + React 19 + TypeScript. Zwei-Bereiche-Layout: Dashboard + Einstellungen.

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [Gateway starten/stoppen] [Status] [=]  |
+------------------------------------------+
|  Dashboard                                |
|  +- LLM-Anbieter wählen ----------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]   ||
|  +- Status ------------------------------+
|  | Port 4000 | API-Schlüssel | Gateway-URL||
|  | Modell-Routing-Tabelle                ||
|  +- Neuestes Log ------------------------+
|  | Log-Anzeige mit Pro/Flash-Zählern     ||
|  +---------------------------------------+
+------------------------------------------+

Einstellungen (=):
  +- Sprache ------------------------------+
  | Dropdown zum sofortigen Wechsel        |
  +- API-Schlüssel ------------------------+
  | Anbieterbezogene API-Schlüsselverwaltung|
  +- Claude Desktop Einrichtung -----------+
  | Config-JSON generieren, kopieren,      |
  | Konfigurationsdatei-Erkennung          |
  +- Gateway-Konfiguration ----------------+
  | config.json-Editor (erweitert)         |
  +---------------------------------------+
```

### Tauri-Befehle

| # | Befehl | Typ | Beschreibung |
|---|--------|-----|--------------|
| 1 | `check_health` | async | Proxy-Gesundheitscheck |
| 2 | `check_gateway_status` | sync | Port 4000 + tokio-Task-Lebendigkeit |
| 3 | `check_api_key` | sync | API-Schlüssel-Status des aktiven Anbieters |
| 4 | `set_env_api_key` | sync | API-Schlüssel über setx speichern |
| 5 | `get_port_4000_process` | sync | PID von Port 4000 via netstat abrufen |
| 6 | `read_config` | sync | config.json lesen |
| 7 | `read_config_raw` | sync | Unformatierter config.json-Text + Kodierungserkennung |
| 8 | `write_config` | sync | config.json speichern (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | Neuestes Log lesen |
| 10 | `read_log` | sync | Angegebene Log-Datei lesen |
| 11 | `list_logs` | sync | Log-Dateien auflisten |
| 12 | `create_new_log` | sync | Neue Log-Datei erstellen |
| 13 | `open_logs_folder` | sync | Log-Ordner öffnen |
| 14 | `open_path` | sync | Beliebigen Pfad öffnen |
| 15 | `find_claude_configs` | sync | Claude Desktop Konfigurationsdateien automatisch erkennen |
| 16 | `start_proxy` | sync | Proxy starten (Config auflösen -> starten -> Port prüfen) |
| 17 | `stop_proxy` | sync | Proxy stoppen (sauberes Herunterfahren) |
| 18 | `proxy_status` | sync | Task-Lebendigkeit prüfen |
| 19 | `check_all_api_keys` | sync | API-Schlüssel-Status aller Anbieter |
| 20 | `update_active_provider` | sync | active_provider speichern |
| 21 | `update_provider_api_key_env` | sync | provider api_key_env speichern |
| 22 | `get_user_language` | sync | Gespeicherte Spracheinstellung abrufen |
| 23 | `set_user_language` | sync | Spracheinstellung speichern |
| 24 | `is_first_run` | sync | Ersten Start erkennen (Vorhandensein von user_prefs.json) |
| 25 | `openrouter_get_models` | async | OpenRouter-Modellkatalog abrufen/zwischenspeichern |
| 26 | `set_model_upstream` | sync | Upstream-Modell + Thinking-Konfiguration + Fähigkeitsflags für ein Gateway-Modell speichern |
| 27 | `update_server_config` | sync | Server-Host/Port/CORS-Einstellungen speichern |
| 28 | `update_normalize_model_identity` | sync | Umschalter zur Normalisierung der Antwortmodell-Identität speichern (aktualisiert config + Laufzeit-AtomicBool) |
| 29 | `update_claude_code_auto_compact_global` | sync | Globale Claude-Code-Kontextverwaltung umschalten (aktiviert + Auslöse-Prozent) |
| 30 | `update_claude_code_auto_compact_target` | sync | Kontextmodus pro Anbieter/Profil festlegen (auto / manual / claude_default) + manuelle Fenster-Token |
| 31 | `update_claude_code_context_settings` | sync | Kombiniertes atomares Update der globalen + Ziel-Kontexteinstellungen |
| 32 | `resolve_claude_code_auto_compact` | sync | Effektive Kontexteinstellungen auflösen (Modus, Fenster-Token, Auslöse-Prozent, Status) |
| 33 | `build_claude_code_launch_command` | sync | Vollständigen PowerShell-Claude-Code-Startbefehl erzeugen (Gateway- + Kontext-Umgebungsvariablen) |

### Proxy-Server (proxy.rs)

In v0.3.0 von Python nach Rust (axum 0.7/reqwest) portiert.

#### Endpunkte

| Methode | Pfad | Verhalten |
|---------|------|-----------|
| GET | `/health` | Gesundheitscheck |
| GET | `/v1/models` | Öffentliche Modellliste (nur `visible: true`) |
| POST | `/v1/messages` | Modellauflösung -> Thinking-Injektion -> Medienprüfung -> Weiterleitung (stream/non-stream) |
| POST | `/v1/messages/count_tokens` | An Upstream weiterleiten, wenn unterstützt |

#### Modell-Routing

Erstellt eine Rückwärtssuche-Tabelle von Gateway-Modell -> (Anbieter, Upstream-Modell) unter Verwendung des `models`-Abschnitts jedes Anbieters. Da alle Anbieter dieselben Gateway-Modellnamen verwenden, gewinnt `active_provider` bei Kollisionen. Effektiv landen nur die Modelle des aktiven Anbieters in der Routing-Tabelle.

#### API-Schlüssel-Validierung (seit v0.5.0)

Durchgang 1: Modell-Routing-Tabelle erstellen (keine API-Schlüssel benötigt)
Durchgang 2: Nur API-Schlüssel für Anbieter prüfen, auf die die Routing-Tabelle verweist

#### Thinking-Injektion

Für Modelle mit `thinking: "disabled"` in ihrem Konfigurationseintrag wird `{"type": "disabled"}` nur injiziert, wenn der Benutzer Thinking nicht explizit gesetzt hat.

#### Normalisierung der Antwortmodell-Identität

Wenn `normalize_response_model_identity` aktiviert ist, schreibt das Proxy das `model`-Feld in Upstream-Antworten neu:

- **Nicht-Streaming**: Parst die JSON-Antwort, schreibt `model` auf den kanonischen Anthropic-Namen um, serialisiert neu
- **Streaming (SSE)**: Fängt `message_start`-Event-Frames ab und schreibt `model` an Ort und Stelle mithilfe von Bytebereich-Ersetzung neu, um SSE-Formatierung und Leerzeichen zu erhalten
- **Überspringungsgründe**: `disabled` (Umschalter aus), `non_success_status` (Nicht-200-Antwort), `content_encoding_not_transformable` (gzip/brotli), `stream_error`, `stream_cancelled`
- **Entscheidungslogik**: Reine Funktionen (`should_normalize_nonstream`, `nonstream_skip_reason`), die sowohl vom Produktionscode als auch von Tests verwendet werden

#### Medienprüfung / Bild-Sanitierung

Modellspezifische `supports_vision` / `supports_video`-Flags bestimmen das Verhalten. Für Nicht-Vision-Modelle, die Bilder empfangen, gilt `non_vision_image_policy`:
- `replace` (Standard): Bildblöcke durch Platzhaltertext ersetzen
- `drop`: Bildblöcke entfernen (Platzhalter einfügen, wenn der Inhalt leer wird)
- `reject`: 400-Fehler zurückgeben

Video-Blöcke geben immer 400 zurück. `non_vision_image_policy` ist über `/health` sichtbar.

#### Claude-Code-Kontextverwaltung

Die Claude-Code-Kontextsteuerung verwendet zwei offizielle Umgebungsvariablen:

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

Auflöse-Pipeline:

1. Jede Standardroute (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) in ihr Upstream-Modell auflösen
2. Die Kontextkapazität jedes Upstream-Modells in `model_context_windows.json` nachschlagen
3. Voraussetzen, dass alle drei Kapazitäten bekannt sind
4. Die kleinste bekannte Kapazität als sicheres Kontextfenster verwenden
5. Den konfigurierten Auslöse-Prozentsatz anwenden

Modi: `auto` (kleinste bekannte Kapazität), `manual` (`window_tokens`), `claude_default` (Claude Codes eigener Standard; keine Variablen gesetzt). Der effektive Status ist `applied`, `disabled` oder `incomplete`.

Der Startbefehl kombiniert die Gateway-Verbindungsvariablen mit den Kontextvariablen:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

Wenn die Kontextsteuerung nicht angewendet wird, entfernt der Befehl zuerst veraltete Variablen:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

Die Prozent-Überschreibung verschiebt die Kompaktierung nur nach vorne; Werte, die die Kompaktierung über Claude Codes Standard hinaus verzögern würden, können ignoriert werden. Anthro Bridge erzeugt und kopiert den Befehl nur — es führt ihn nie aus, und das beweist nicht, dass eine bestimmte Claude-Code-Version die Variablen berücksichtigt (die endgültige Bestätigung erfordert Claude-Code-Diagnosen oder beobachtetes Kompaktierungsverhalten).

### Mehrsprachigkeit

Datei-pro-Sprache-Architektur mit `import.meta.glob`-Auto-Discovery:

```
gui/src/i18n/lang/
  en.ts      Englisch (kanonisch — definiert den TranslationKey-Typ)
  ja.ts      Japanisch
  zh-CN.ts   Chinesisch (Vereinfacht)
  zh-TW.ts   Chinesisch (Traditionell)
  ko.ts      Koreanisch
  fr.ts      Französisch
  de.ts      Deutsch
  es.ts      Spanisch
```

Um eine Sprache hinzuzufügen: `en.ts` kopieren, übersetzen, neu bauen. Keine Code-Änderungen erforderlich.

### config.json Referenz

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key env var name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "real-model-name" },
      "visible_models": ["claude-public-model-name"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "real-model-name",
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

Jeder Anbieter oder jedes OpenRouter-Profil kann außerdem über `claude_code: { "auto_compact": { "mode": "auto" } }` einen Standard-Kontextmodus festlegen. Der effektive Modus für eine Route ist der Anbieter-/Profilwert, der auf den globalen Block zurückfällt; `resolve_claude_code_auto_compact` gibt das aufgelöste Ergebnis zurück.
