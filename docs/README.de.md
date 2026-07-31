[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

Anthro Bridge ist ein lokales Gateway und Desktop-Konfigurationswerkzeug, das es Claude Desktop und Claude Code ermoglicht, mehrere Drittanbieter-LLM-Anbieter uber eine Anthropic-kompatible API zu nutzen.

Die Anwendung besteht aus:

- Einem lokalen Proxy-Server, geschrieben in Rust
- Einer nativen Windows-GUI, erstellt mit Tauri 2, React und TypeScript
- Modellbasiertem Routing von Anthropic-Modellnamen zu anbieterspezifischen Upstream-Modellen
- Routenbezogener Konfiguration von Modell, Reasoning und Fahigkeiten

Anthro Bridge ist ein unabhangiges Projekt. Es ist weder ein Fork, Frontend noch eine Begleitanwendung fur Moon Bridge.

## Unterstutzte Modelle

Anthro Bridge unterstutzt zwei Kategorien von Upstream-Modellen.

### Native Integrationen

Diese Anbieter werden uber ihre eigenen Anthropic-kompatiblen APIs unterstutzt. Es ist kein OpenRouter-Konto erforderlich.

| Anbieter | Unterstutzte Modellfamilien | Verbindung |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro und V4 Flash | Direkte Anbieter-API |
| MiniMax | MiniMax M3 und M2.7 Varianten | Direkte Anbieter-API |
| Kimi / Moonshot | Kimi K2.x und Kimi K3 | Direkte Anbieter-API |
| MiMo / Xiaomi | MiMo V2.5 und V2.5 Pro Varianten | Direkte Anbieter-API |

### Uber OpenRouter unterstutzte Modelle

Diese Modelle werden uber ein OpenRouter-Profil angesprochen. Jedes Profil hat seinen eigenen API-Schlussel, Routenzuordnungen und Reasoning-Einstellungen.

| Anbieter oder Modellfamilie | Integrierte Unterstutzung | Reasoning-Steuerung |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Ja | Modellspezifische Thinking-Steuerung |
| Tencent Hy3 | Ja | Niedriger und hoher Reasoning-Aufwand |
| InclusionAI Ring | Ja | Modellspezifische Thinking- und Reasoning-Steuerung |
| StepFun Step 3.5 / Step 3.7 | Ja | Niedrig, Mittel und Hoch, sofern unterstutzt |
| InclusionAI Ling-Familie | Ja | Modellspezifische Thinking-Steuerung |

Andere OpenRouter-Modelle konnen ebenfalls aus der Live-OpenRouter-Modellliste ausgewahlt oder manuell eingegeben werden. Integrierte Unterstutzung bedeutet, dass Anthro Bridge die Modellfamilie, Fahigkeitsflags, Anbietergruppierung und das Reasoning-Steuerungsverhalten bereits kennt.

## Funktionsweise

Claude Desktop und Claude Code senden Anfragen unter Verwendung von Anthropic-Modellnamen wie:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge behandelt diese Namen als stabile Routenbezeichner. Die GUI legt fest, welcher Anbieter und welches Upstream-Modell von jeder Route verwendet wird.

Beispiel:

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

Nur Felder, die fur den Upstream-Anbieter angepasst werden mussen, werden geandert. Nachrichten, Tool-Aufrufe, Tool-Ergebnisse, Thinking-Blocke und Streaming-Daten bleiben ansonsten erhalten, sofern die Upstream-API sie unterstutzt.

## Hauptfunktionen

### Anbieter-Routing

Anthro Bridge unterstutzt zwei Upstream-Verbindungstypen:

1. **Direkte Anbieterintegrationen**, die sich mit der Anthropic-kompatiblen API eines Anbieters verbinden.
2. **OpenRouter-Profile**, die sich mit OpenRouter verbinden und uber eine einzige API an mehrere Anbieter und Modellfamilien routen konnen.

#### Direkte Anbieterintegrationen

| Anbieter-ID | Anzeigename | Standard-Endpunkt |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter-Integration

| Verbindungstyp | Anzeigename | Endpunkt |
|---|---|---|
| Multi-Profil-Modellgateway | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter wird nicht als einzelner Modellanbieter behandelt. Jedes OpenRouter-Profil kann unabhangig Modelle aus unterstutzten Anbietergruppen wie Poolside, Tencent, InclusionAI und StepFun sowie andere Modelle auswahlen, die uber die OpenRouter-API entdeckt oder manuell eingegeben werden.

Jede Anthropic-Route kann unabhangig entweder einem Direktanbieter-Modell oder einem uber ein OpenRouter-Profil ausgewahlten Modell zugeordnet werden.

### OpenRouter Multi-Profil-Unterstutzung

Mehrere OpenRouter-Profile konnen unabhangig voneinander erstellt und verwaltet werden.

Jedes Profil besitzt:

- Einen Profilnamen
- Eine API-Schlussel-Konfiguration
- Opus-, Sonnet- und Haiku-Routenzuordnungen
- Thinking- oder Reasoning-Einstellungen
- Eine zwischengespeicherte OpenRouter-Modellliste

Profile konnen uber die GUI hinzugefugt, umbenannt, geloscht und ausgewahlt werden.

Integrierte OpenRouter-Anbietergruppen umfassen derzeit Poolside, Tencent, InclusionAI, StepFun und andere bekannte Modellfamilien. Unbekannte Modelle bleiben uber die Suche oder benutzerdefinierte Modelleingabe verfugbar.

### Modell- und Reasoning-Steuerung

Die verfugbaren Steuerelemente hangen vom ausgewahlten Modell ab.

Unterstutzte Steuerelemente konnen Folgendes umfassen:

- Thinking ein oder aus
- Normale, niedrige, mittlere, hohe, sehr hohe oder maximale Reasoning-Modi
- Anbieterspezifischer Reasoning-Aufwand
- Fest eingestellte Reasoning-Modi fur Modelle, die keine Benutzerauswahl erlauben

Beim Wechseln von Modellen versucht Anthro Bridge, die am besten kompatible Reasoning-Einstellung beizubehalten. Wenn die exakt vorherige Einstellung nicht verfugbar ist, wird die nachstgelegene unterstutzte Option ausgewahlt, wobei bei zwei gleich nahen Optionen die schwuchere bevorzugt wird.

### Fahigkeitserkennung

Anthro Bridge kombiniert eine integrierte Fahigkeitsregistrierung mit Live-OpenRouter-Metadaten.

Fahigkeiten konnen Folgendes umfassen:

- Bildeingabe
- Videoeingabe
- Thinking-Unterstutzung
- Reasoning-Effort-Unterstutzung
- Bekannte Preisgestaltung
- Anbieterspezifische Anfrageubersetzungsregeln

Live-OpenRouter-Metadaten werden zwischengespeichert, um unnotige API-Aufrufe zu reduzieren.

### Antwortmodell-Normalisierung

Upstream-APIs geben haufig ihren eigenen Modellnamen in Antworten zuruck. Anthro Bridge kann dieses Feld zuruck in den vom Client erwarteten Anthropic-Routennamen umschreiben.

Zum Beispiel:

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

Die Normalisierung gilt sowohl fur Streaming- als auch fur Nicht-Streaming-Antworten und kann in den Einstellungen aktiviert oder deaktiviert werden.

### Serialisierte Konfigurationsschreibvorgange

Konfigurationsanderungen werden serialisiert, um zu verhindern, dass gleichzeitige Schreibvorgange Einstellungen beschadigen oder zurucksetzen.

Dies betrifft Vorgange wie:

- Modellanderungen
- Anderungen des Thinking-Modus
- Anderungen des Reasoning-Aufwands
- Anderungen an OpenRouter-Profilen
- API-Schlussel-bezogene Konfigurationsanderungen

### OpenRouter-Speicherwarteschlange

OpenRouter-Routenanderungen werden uber eine dedizierte Speicherwarteschlange verarbeitet.

Die Warteschlange bietet:

- Serialisierte Speichervorgange
- Uberschreibung veralteter Anfragen
- Routenidentitat, die bei Einreichung einer Anfrage erfasst wird
- Schutz vor veralteten React-Closures
- Schutz vor Zurucksetzung durch eine zuvor ausgewahlte Route
- Wiederholung der Aktualisierung nach erfolgreichem Speichern
- Aggregierte Gateway-Neustartbehandlung
- Sichere Verarbeitung von Anfragen, die wahrend der Nachspeicherarbeit hinzugefugt wurden

Dies verhindert, dass schnelle Modellwechsel, Routenumschaltungen oder verzogerte Tauri-Antworten alte UI-Werte wiederherstellen.

### Gateway-Verwaltung

Die GUI bietet:

- Gateway-Start- und -Stopp-Steuerung
- Anbieter- und Profilauswahl
- Routenkonfiguration
- API-Schlusselverwaltung
- Protokollansicht
- Aktualisierung der Modellliste
- Speicherstatus- und Fehleranzeige

Das Gateway lauscht auf:

```text
http://127.0.0.1:4000
```

## Voraussetzungen

- Windows 10 oder Windows 11
- Node.js 24 oder hoher fur die Entwicklung
- Stabile Rust-Toolchain fur die Entwicklung
- Ein API-Schlussel fur mindestens einen unterstutzten Anbieter

Ein einziger Anbieterschlussel ist ausreichend. Sie benotigen keine Schlussel fur jeden Anbieter.

## Installation

Laden Sie den neuesten Windows-Installer von der Projekt-Releases-Seite herunter und fuhren Sie ihn aus.

Der Installer unterstutzt:

- Englisch
- Japanisch
- Vereinfachtes Chinesisch
- Traditionelles Chinesisch
- Koreanisch
- Franzosisch
- Deutsch
- Spanisch

Um Anthro Bridge zu aktualisieren, fuhren Sie den neueren Installer aus. Bestehende Benutzereinstellungen bleiben erhalten.

Die stabile Benutzerkonfiguration wird gespeichert unter:

```text
%APPDATA%\Anthro Bridge\
```

Entwicklungs-Builds verwenden eine separate Anwendungsidentitat und Datenverzeichnis:

```text
%APPDATA%\Anthro Bridge Dev\
```

Dies ermoglicht die Koexistenz von stabilen und Entwicklungsversionen ohne gemeinsame Konfigurations- oder Cache-Dateien.

## Schnellstart

### 1. API-Schlussel konfigurieren

Offnen Sie:

```text
Einstellungen > API-Schlussel
```

Geben Sie den Schlussel fur den Anbieter ein, den Sie verwenden mochten, und speichern Sie ihn.

Gangige Umgebungsvariablennamen sind:

| Anbieter | Umgebungsvariable |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter-Profile konnen profilspezifische Schlusseleinstellungen verwenden, die uber die GUI verwaltet werden.

### 2. Routenmodelle konfigurieren

Offnen Sie die Einstellungen und wahlen Sie das Upstream-Modell fur jede Route aus:

- Opus
- Sonnet
- Haiku

Wahlen oder erstellen Sie fur OpenRouter zuerst ein Profil und konfigurieren Sie dann jede Route innerhalb dieses Profils.

### 3. Gateway starten

Klicken Sie auf **Gateway starten**.

Uberprufen Sie, ob der lokale Endpunkt verfugbar ist:

```text
GET http://127.0.0.1:4000/health
```

### 4. Claude Desktop oder Claude Code konfigurieren

Richten Sie den Client auf den Anthro Bridge-Endpunkt, wahrend Sie weiterhin Anthropic-Modellnamen verwenden.

Detaillierte Anleitungen zur Drittanbieter-Inferenz finden Sie in:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API-Endpunkte

| Methode | Pfad | Beschreibung |
|---|---|---|
| `GET` | `/health` | Gateway-Health-Check |
| `GET` | `/v1/models` | Offentliche Routen-Modellliste |
| `POST` | `/v1/messages` | Streaming- und Nicht-Streaming-Messages-API |
| `POST` | `/v1/messages/count_tokens` | Token-Zahlung, wenn vom ausgewahlten Anbieter unterstutzt |

## Konfiguration

Die Hauptkonfigurationsdatei ist `config.json`.

Die meisten Einstellungen sollten uber die GUI geandert werden. Manuelle Bearbeitung ist fur fortgeschrittene Anwender vorgesehen.

Wichtige Modellfelder umfassen:

| Schlussel | Beschreibung |
|---|---|
| `models.<route>.upstream_model` | Upstream-Modellname, der an den Anbieter gesendet wird |
| `models.<route>.thinking_mode` | Routen-spezifischer Thinking-Modus |
| `models.<route>.reasoning_effort` | Anbieterspezifischer Reasoning-Aufwand |
| `models.<route>.supports_vision` | Bildunterstutzungs-Override |
| `models.<route>.supports_video` | Videounterstutzungs-Override |
| `models.<route>.visible` | Ob die Route fur Clients und das Dashboard sichtbar ist |
| `non_vision_image_policy` | Wie nicht unterstutzte Bildeingaben behandelt werden |
| `normalize_response_model_identity` | Ob Antwort-Modellnamen normalisiert werden |

Nicht unterstutzte Bilder konnen durch eine der folgenden Richtlinien behandelt werden:

- `replace`: das Bild durch einen Textplatzhalter ersetzen
- `drop`: den Bildinhalt entfernen
- `reject`: einen Fehler zuruckgeben

## Anbieterhinweise

### DeepSeek

DeepSeek Pro-Modelle konnen konfigurierbaren Reasoning-Aufwand verwenden. Flash-Modelle bieten nicht dieselbe Reasoning-Effort-Steuerung, daher werden nicht verfugbare Optionen automatisch deaktiviert.

### MiniMax

Das Verhalten von MiniMax-Modellen unterscheidet sich je nach Modellgeneration. Anthro Bridge wendet das vom ausgewahlten Modell geforderte Anfrageformat an, einschlie�?lich adaptivem oder deaktiviertem Thinking, sofern unterstutzt.

### Kimi

Kimi-Modelle konnen je nach Modellfamilie entweder einen Thinking-Parameter oder einen festen Reasoning-Effort-Modus verwenden. Anthro Bridge ubersetzt die GUI-Auswahl in das entsprechende Upstream-Anfrageformat.

### MiMo

MiMo verwendet `thinking_mode` anstelle des generischen `thinking`-Feldes fur unterstutzte Routen.

Die Vision-Unterstutzung variiert je nach Modell. Anthro Bridge wendet die konfigurierte Richtlinie fur nicht unterstutzte Bilder an, wenn eine Route keine Bildeingabe akzeptieren kann.

### OpenRouter

OpenRouter-Modelle werden nach Anbieter gruppiert, sofern sie erkannt werden. Die GUI bietet:

- Modellsuche
- Anbietergruppierung
- Benutzerdefinierte Modelleingabe
- Fahigkeitsabzeichen
- Preisanzeige
- Modellbezogene Reasoning-Steuerung
- Einheitliche Aktualisierung der Modellliste

OpenRouter-Modellfahigkeiten und -verhalten konnen sich im Laufe der Zeit andern. Live-Metadaten werden verwendet, wo verfugbar, wahrend die integrierte Registrierung stabile Standardwerte fur bekannte Modelle bereitstellt.

### Poolside Laguna

Laguna S und Laguna XS verwenden OpenRouter-Reasoning-Ubersetzungsregeln.

Anthro Bridge erkennt auch ein Fehlermuster, bei dem eine Antwort das Ausgabe-Token-Limit erreicht, nachdem nur Reasoning-Inhalt und kein nutzbarer Text oder Tool-Aufruf produziert wurde. Wenn dies erkannt wird, wird das Ereignis protokolliert, damit der Benutzer die Ausgabelimits anpassen, Thinking deaktivieren oder ein anderes Modell wahlen kann.

## Benutzeroberflache

Die Einstellungsoberflache umfasst:

- Einklappbare Anbieterabschnitte
- Opus-, Sonnet- und Haiku-Routenkonfiguration
- Modellsuche und Anbietergruppierung fur OpenRouter
- Thinking- und Reasoning-Steuerung basierend auf den Modellfahigkeiten
- Benutzerdefinierte Upstream-Modelleingabe
- Automatisches Speichern von Routen
- Explizites Speichern des API-Schlussels
- Speicherfortschritt und Fehlermeldungen
- Informationen zu Modellpreisen und -fahigkeiten
- Umschalter fur Antwortmodell-Normalisierung

Das Dashboard umfasst:

- Anbieter- oder OpenRouter-Profilauswahl
- Gateway-Status
- Aktuelle Routenzuordnungen
- Fahigkeitsindikatoren
- Preisinformationen
- Anbieterwechselstatus

## Entwicklung

### Projektstruktur

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   └── package.json
└── LICENSE
```

### Im Entwicklungsmodus ausfuhren

```bash
cd gui
npm install
npm run tauri dev
```

### Entwicklungsvariante erstellen

Verwenden Sie unter Windows einen einzelnen Rust-Build-Job, um intermittierende Compiler-Abbruche zu vermeiden:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Entwicklungs-Builds verwenden:

- Fenstertitel: `Anthro Bridge (DEV)`
- Port: `4000`
- Anwendungsidentitat: `com.soheidon.anthro-bridge.dev`
- Separate Konfigurations- und Cache-Verzeichnisse

### Stabile Builds

Stabile Builds sollten nur zur Release-Vorbereitung erstellt werden. Normale Implementierungs- und Verifizierungsarbeiten sollten die Entwicklungsvariante verwenden.

## Verifizierung

Frontend-Verifizierung:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rust-Verifizierung:

```bash
cd gui/src-tauri
cargo check
```

Speziell fur die OpenRouter-Routenauswahl:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

Die OpenRouter-Auswahltests decken Folgendes ab:

- Erfasste Routenidentitat wahrend gespeicherter Warteschlangenvorgange
- Routenubergreifender Zurucksetzungsschutz
- Schutz vor veralteten Callbacks
- Verhalten bei Wiederholung der Aktualisierung
- Gateway-Neustart nach fehlgeschlagener Aktualisierung
- Uberschreibung laufender Anfragen
- Generationsbasierte Zurucksetzungsunterdruckung

Ein dedizierter Multi-Save-Test fur die Neustart-Aggregation kann hinzugefugt werden, um das folgende Verhalten abzusichern:

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## Manuelle Verifizierungs-Checkliste

Automatisierte Tests bilden nicht jede Tauri- und React-Timing-Bedingung ab. Uberprufen Sie vor der Veroffentlichung Folgendes im Entwicklungs-Build:

- Jedes OpenRouter-Profil zeigt die korrekten Hover-Details
- Die Modellauswahl wird nach einer Anderung nicht sichtbar zuruckgesetzt
- Thinking- und Reasoning-Auswahlen bleiben nach dem Speichern stabil
- Einstellungen bleiben nach dem Schlie�?en und erneuten Offnen des Einstellungsbildschirms korrekt
- Einstellungen bleiben nach dem Neustart der Anwendung korrekt
- Das Wechseln von Profilen wahrend eines Speichervorgangs beschadigt keines der Profile
- Ein fehlgeschlagener Speichervorgang setzt nur die Route zuruck, die ihn ausgelost hat
- Ein erfolgreicher Wiederholungsversuch der Aktualisierung loscht den vorherigen Fehler
- Ein fehlgeschlagener Wiederholungsversuch der Aktualisierung lasst den letzten Fehler sichtbar
- Der erforderliche Gateway-Neustart erfolgt einmal nach dem Stapel
- Benutzerdefinierte Modelle werden korrekt gespeichert und neu geladen
- Integrierte und Live-OpenRouter-Fahigkeiten werden korrekt angezeigt

## Fehlerbehebung

### Port 4000 wird bereits verwendet

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Ein Modell lehnt Bild- oder Videoeingabe ab

Modellfahigkeiten variieren je nach Anbieter und Route. Uberprufen Sie die Fahigkeitsabzeichen in der GUI und wahlen Sie eine kompatible Route.

Fur nicht unterstutzte Bildeingaben folgt Anthro Bridge der `non_vision_image_policy`.

### Einstellungen werden nach einem Upgrade zuruckgesetzt

Starten Sie die Anwendung zuerst neu, damit Migrationen ausgefuhrt werden konnen.

Wenn das Problem weiterhin besteht:

1. Sichern Sie die Benutzerkonfiguration.
2. Vergleichen Sie sie mit der gebundelten Konfiguration.
3. Entfernen Sie veraltete Felder oder setzen Sie die Benutzerkonfiguration bei Bedarf zuruck.

Speicherort der stabilen Konfiguration:

```text
%APPDATA%\Anthro Bridge\config.json
```

Speicherort der Entwicklungskonfiguration:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter-Modellliste ist veraltet

Verwenden Sie die einheitliche Modellaktualisierungssteuerung in den Einstellungen. Anthro Bridge speichert Modellmetadaten zwischen, sodass nach einer Anderung eines Modelleintrags durch OpenRouter moglicherweise eine manuelle Aktualisierung erforderlich ist.

## Ubersetzung

Englisch ist die Quell-README.

Ubersetzte README-Dateien werden unter `docs/` gespeichert. Wenn sich die englische README andert, generieren oder aktualisieren Sie die ubersetzten Dateien aus der englischen Quelle, anstatt jede Sprache unabhangig zu bearbeiten.

Sprachdateien fur die Anwendungsoberflache werden gespeichert unter:

```text
gui/src/i18n/lang/
```

## Lizenz

MIT-Lizenz. Siehe [LICENSE](../LICENSE).
