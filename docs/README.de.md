[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Aktuelle Version: 0.16.0**

Anthro Bridge ist ein lokales Gateway und Desktop-Konfigurationswerkzeug, das es Claude Desktop und Claude Code ermöglicht, mehrere LLM-Anbieter von Drittanbietern über eine Anthropic-kompatible API zu nutzen.

Die Anwendung besteht aus:

- Einem lokalen Proxy-Server, geschrieben in Rust
- Einer nativen Windows-GUI, erstellt mit Tauri 2, React und TypeScript
- Modellbasiertem Routing von Anthropic-Modellnamen zu anbieterspezifischen Upstream-Modellen
- Routenbezogener Konfiguration von Modell, Reasoning und Fähigkeiten

Anthro Bridge ist ein unabhängiges Projekt. Es ist weder ein Fork, Frontend noch eine Begleitanwendung für Moon Bridge.

## Neuerungen der Version 0.16.0

Version 0.16.0 ergänzt die modellbewusste Kontextverwaltung für Claude Code.

- Anthro Bridge ermittelt die Kontextkapazität der Upstream-Modelle, die den Routen Opus, Sonnet und Haiku zugewiesen sind.
- Im automatischen Modus wird die kleinste bekannte Kapazität über die drei Routen hinweg als sicheres Claude-Code-Kontextfenster verwendet.
- Die Kontextsteuerung wird nur angewendet, wenn alle drei Routenkapazitäten bekannt sind.
- Die Kopfzeile bietet einen kompakten Umschalter für die Kontextverwaltung; erweiterter Modus und Schwellenwerte bleiben über `config.json` verfügbar.
- Die Anwendung kann einen vollständigen PowerShell-Startbefehl generieren, der die Verbindungsvariablen von Anthro Bridge und die Kontextsteuerungsvariablen von Claude Code enthält.
- Wenn die Kontextverwaltung deaktiviert oder unvollständig ist, entfernt der generierte Befehl veraltete Kontextsteuerungsvariablen aus der aktuellen PowerShell-Sitzung.
- Die integrierten Kontextmetadaten umfassen die Standardmodelle der Direktanbieter und die integrierten OpenRouter-Modelle.
- Der generierte Befehl und sein Verhalten bei Umgebungsvariablen werden durch Rust-Unit-Tests, Windows-PowerShell-Integrationstests und Tests des Frontend-Kopierablaufs abgedeckt.

## Unterstützte Modelle

Anthro Bridge unterstützt zwei Kategorien von Upstream-Modellen.

### Native Integrationen

Diese Anbieter werden über ihre eigenen Anthropic-kompatiblen APIs unterstützt. Es ist kein OpenRouter-Konto erforderlich.

| Anbieter | Unterstützte Modellfamilien | Verbindung |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro und V4 Flash | Direkte Anbieter-API |
| MiniMax | MiniMax M3- und M2.7-Varianten | Direkte Anbieter-API |
| Kimi / Moonshot | Kimi K2.x und Kimi K3 | Direkte Anbieter-API |
| MiMo / Xiaomi | MiMo V2.5- und V2.5-Pro-Varianten | Direkte Anbieter-API |

### Über OpenRouter unterstützte Modelle

Diese Modelle werden über ein OpenRouter-Profil angesprochen. Jedes Profil hat seinen eigenen API-Schlüssel, seine eigenen Routenzuordnungen und Reasoning-Einstellungen.

| Anbieter oder Modellfamilie | Integrierte Unterstützung | Reasoning-Steuerung |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Ja | Modellspezifische Thinking-Steuerung |
| Tencent Hy3 | Ja | Niedriger und hoher Reasoning-Aufwand |
| InclusionAI Ring | Ja | Modellspezifische Thinking- und Reasoning-Steuerung |
| StepFun Step 3.5 / Step 3.7 | Ja | Niedrig, Mittel und Hoch, sofern unterstützt |
| InclusionAI Ling-Familie | Ja | Modellspezifische Thinking-Steuerung |
| OpenAI GPT-5.6 Sol / Terra / Luna | Ja | Modellspezifische Thinking- und Reasoning-Steuerung |

Andere OpenRouter-Modelle können ebenfalls aus der Live-OpenRouter-Modellliste ausgewählt oder manuell eingegeben werden. Integrierte Unterstützung bedeutet, dass Anthro Bridge die Modellfamilie, Fähigkeitsflags, Anbietergruppierung und das Verhalten der Reasoning-Steuerung bereits kennt.

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

Nur Felder, die für den Upstream-Anbieter angepasst werden müssen, werden geändert. Nachrichten, Tool-Aufrufe, Tool-Ergebnisse, Thinking-Blöcke und Streaming-Daten bleiben ansonsten erhalten, sofern die Upstream-API sie unterstützt.

## Hauptfunktionen

### Anbieter-Routing

Anthro Bridge unterstützt zwei Upstream-Verbindungstypen:

1. **Direkte Anbieterintegrationen**, die sich mit der Anthropic-kompatiblen API eines Anbieters verbinden.
2. **OpenRouter-Profile**, die sich mit OpenRouter verbinden und über eine einzige API an mehrere Anbieter und Modellfamilien routen können.

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

OpenRouter wird nicht als einzelner Modellanbieter behandelt. Jedes OpenRouter-Profil kann unabhängig Modelle aus unterstützten Anbietergruppen wie Poolside, Tencent, InclusionAI und StepFun sowie andere Modelle auswählen, die über die OpenRouter-API entdeckt oder manuell eingegeben werden.

Jede Anthropic-Route kann unabhängig entweder einem Direktanbieter-Modell oder einem über ein OpenRouter-Profil ausgewählten Modell zugeordnet werden.

### OpenRouter-Multi-Profil-Unterstützung

Mehrere OpenRouter-Profile können unabhängig voneinander erstellt und verwaltet werden.

Jedes Profil besitzt:

- Profilname
- API-Schlüssel-Konfiguration
- Opus-, Sonnet- und Haiku-Routenzuordnungen
- Thinking- oder Reasoning-Einstellungen
- Zwischengespeicherte OpenRouter-Modellliste

Profile können über die GUI hinzugefügt, umbenannt, gelöscht, per Drag-and-Drop neu angeordnet, ausgeblendet und ausgewählt werden. Das Dashboard zeigt für jedes sichtbare Profil eine Karte und behält die gespeicherte Reihenfolge nach einer Aktualisierung bei.

Integrierte OpenRouter-Anbietergruppen umfassen derzeit Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6 und andere erkannte Modellfamilien. Unbekannte Modelle bleiben über die Suche oder benutzerdefinierte Modelleingabe verfügbar. Das Dashboard kürzt anbieterqualifizierte IDs wie `poolside/laguna-s-2.1` aus Gründen der Lesbarkeit zu `laguna-s-2.1`, behält die vollständige ID jedoch für das Routing bei.

### OpenRouter-Preise und Modelldetails

Das Modellpreis-Panel der Einstellungen zeigt integrierte Preise für unterstützte OpenRouter-Modelle, einschließlich Preisen für Eingabe, Ausgabe und zwischengespeicherte Eingabe. Aktionspreise können zusammen mit überarbeiteten Standardpreisen angezeigt werden, einschließlich der GPT-5.6-Sol-, -Terra- und -Luna-Varianten sowie ihrer Pro-Varianten. Preisnotizen können gegebenenfalls Langkontext-Preise enthalten.

### Responsive Dashboard-Größenanpassung

Die anfängliche Fensterhöhe wird aus der Anzahl der sichtbaren Anbieter- und OpenRouter-Karten im dreispaltigen Dashboard berechnet. Zusätzliche Kartenzeilen erhöhen die Fensterhöhe unter Berücksichtigung der nativen Mindestgröße, des Monitor-Arbeitsbereichs, der DPI-Skalierung und der Titelleistendekorationen. Wenn sich Sichtbarkeit oder Anzahl der Profile ändert, wird die Höhe für die neue Zeilenanzahl neu berechnet; manuelle Größenänderungen bleiben erhalten, solange die Zeilenanzahl unverändert bleibt.

### Lokalisierter Windows-Installer

Der Windows-NSIS-Installer bietet die Sprachauswahl für Englisch, Japanisch, vereinfachtes Chinesisch, traditionelles Chinesisch, Koreanisch, Französisch, Deutsch und Spanisch. Der Installer verwendet das Anwendungssymbol von Anthro Bridge und erhält die stabile Benutzerkonfiguration während Upgrades.

### Neueste Zuverlässigkeitsverbesserungen der UI

Konfigurationsschreibvorgänge werden serialisiert, OpenRouter-Speichervorgänge verwenden einen Warteschlangen-Aktualisierungspfad mit Schutz vor veralteten Anfragen, und Profil-Neuanordnungsvorgänge erholen sich nach fehlgeschlagenen Aktualisierungen sauber. Regressionstests decken die Profilreihenfolge, Speicherwettläufe, Modellpreise, die Dashboard-Kartenzählung und die Fenstergrößenanpassung ab.

### Modell- und Reasoning-Steuerung

Die verfügbaren Steuerelemente hängen vom ausgewählten Modell ab.

Unterstützte Steuerelemente können Folgendes umfassen:

- Thinking ein oder aus
- Normale, niedrige, mittlere, hohe, sehr hohe oder maximale Reasoning-Modi
- Anbieterspezifischer Reasoning-Aufwand
- Fest eingestellte Reasoning-Modi für Modelle, die keine Benutzerauswahl erlauben

Beim Wechseln von Modellen versucht Anthro Bridge, die am besten kompatible Reasoning-Einstellung beizubehalten. Wenn die exakt vorherige Einstellung nicht verfügbar ist, wird die nächstgelegene unterstützte Option ausgewählt, wobei bei zwei gleich nahen Optionen die schwächere bevorzugt wird.

### Fähigkeitserkennung

Anthro Bridge kombiniert eine integrierte Fähigkeitsregistrierung mit Live-OpenRouter-Metadaten.

Fähigkeiten können Folgendes umfassen:

- Bildeingabe
- Videoeingabe
- Thinking-Unterstützung
- Unterstützung des Reasoning-Aufwands
- Bekannte Preisgestaltung
- Anbieterspezifische Anfrageübersetzungsregeln

Live-OpenRouter-Metadaten werden zwischengespeichert, um unnötige API-Aufrufe zu reduzieren.

### Antwortmodell-Normalisierung

Upstream-APIs geben in Antworten häufig ihren eigenen Modellnamen zurück. Anthro Bridge kann dieses Feld zurück in den vom Client erwarteten Anthropic-Routennamen umschreiben.

Zum Beispiel:

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

Die Normalisierung gilt sowohl für Streaming- als auch für Nicht-Streaming-Antworten und kann in den Einstellungen aktiviert oder deaktiviert werden.

### Serialisierte Konfigurationsschreibvorgänge

Konfigurationsänderungen werden serialisiert, um zu verhindern, dass gleichzeitige Schreibvorgänge Einstellungen beschädigen oder zurücksetzen.

Dies betrifft Vorgänge wie:

- Modelländerungen
- Änderungen des Thinking-Modus
- Änderungen des Reasoning-Aufwands
- Änderungen an OpenRouter-Profilen
- API-Schlüssel-bezogene Konfigurationsänderungen

### OpenRouter-Speicherwarteschlange

OpenRouter-Routenänderungen werden über eine dedizierte Speicherwarteschlange verarbeitet.

Die Warteschlange bietet:

- Serialisierte Speichervorgänge
- Überschreibung veralteter Anfragen
- Routenidentität, die bei Einreichung einer Anfrage erfasst wird
- Schutz vor veralteten React-Closures
- Schutz vor Zurücksetzung durch eine zuvor ausgewählte Route
- Wiederholung der Aktualisierung nach erfolgreichem Speichern
- Aggregierte Gateway-Neustartbehandlung
- Sichere Verarbeitung von Anfragen, die während der Nachspeicherarbeit hinzugefügt wurden

Dies verhindert, dass schnelle Modellwechsel, Routenumschaltungen oder verzögerte Tauri-Antworten alte UI-Werte wiederherstellen.

### Kontextverwaltung für Claude Code

Anthro Bridge 0.16.0 kann Claude-Code-Startbefehle mit modellbewussten Kontexteinstellungen generieren.

Der Resolver führt die folgenden Schritte aus:

1. Ermitteln Sie das Upstream-Modell, das jeder kanonischen Route zugewiesen ist:
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. Schlagen Sie die bekannte Kontextkapazität für jedes Upstream-Modell nach.
3. Alle drei Routenkapazitäten müssen bekannt sein.
4. Verwenden Sie die kleinste Kapazität als sicheres Kontextfenster.
5. Wenden Sie den konfigurierten Auslöse-Prozentsatz an.

Wenn die drei Routen beispielsweise Kapazitäten von 1.000.000, 262.144 und 1.000.000 Tokens ergeben, verwendet Anthro Bridge:

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

Der generierte PowerShell-Befehl verwendet die offiziellen Claude-Code-Variablen:

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

Er enthält außerdem die Gateway-Verbindungsvariablen von Anthro Bridge:

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

Beispiel:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

Wenn die Kontextverwaltung deaktiviert ist, auf das Standardverhalten von Claude Code eingestellt ist oder unvollständig ist, weil eine Routenkapazität unbekannt ist, löscht der generierte Befehl veraltete Kontextvariablen, bevor er Claude Code startet:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

Der prozentuale Override fordert eine frühere proaktive Komprimierung an. Claude Code ignoriert möglicherweise Werte, die die Komprimierung über sein eigenes Standardverhalten hinaus verzögern würden.

Anthro Bridge verifiziert die Befehlserzeugung und die Injektion von Umgebungsvariablen in PowerShell. Das allein beweist nicht, dass eine bestimmte Claude-Code-Version die Variablen übernommen hat; die endgültige Bestätigung erfordert Claude-Code-Diagnosen oder die Beobachtung des Komprimierungsverhaltens.

### Gateway-Verwaltung

Die GUI bietet:

- Gateway-Start- und -Stopp-Steuerung
- Anbieter- und Profilauswahl
- Routenkonfiguration
- API-Schlüsselverwaltung
- Protokollansicht
- Aktualisierung der Modellliste
- Speicherstatus- und Fehleranzeige

Das Gateway lauscht auf:

```text
http://127.0.0.1:4000
```

## Voraussetzungen

- Windows 10 oder Windows 11
- Node.js 24 oder höher für die Entwicklung
- Stabile Rust-Toolchain für die Entwicklung
- Ein API-Schlüssel für mindestens einen unterstützten Anbieter

Ein einziger Anbieterschlüssel ist ausreichend. Sie benötigen keine Schlüssel für jeden Anbieter.

## Installation

Laden Sie den neuesten Windows-Installer von der Releases-Seite des Projekts herunter und führen Sie ihn aus.

Der Installer unterstützt:

- Englisch
- Japanisch
- Vereinfachtes Chinesisch
- Traditionelles Chinesisch
- Koreanisch
- Französisch
- Deutsch
- Spanisch

Um Anthro Bridge zu aktualisieren, führen Sie den neueren Installer aus. Bestehende Benutzereinstellungen bleiben erhalten.

Die stabile Benutzerkonfiguration wird gespeichert unter:

```text
%APPDATA%\Anthro Bridge\
```

Entwicklungs-Builds verwenden eine separate Anwendungsidentität und ein separates Datenverzeichnis:

```text
%APPDATA%\Anthro Bridge Dev\
```

Dies ermöglicht die Koexistenz von stabilen Versionen und Entwicklungsversionen ohne gemeinsame Konfigurations- oder Cache-Dateien.

## Schnellstart

### 1. API-Schlüssel konfigurieren

Öffnen Sie:

```text
Settings > API Key
```

Geben Sie den Schlüssel für den Anbieter ein, den Sie verwenden möchten, und speichern Sie ihn.

Gängige Umgebungsvariablennamen sind:

| Anbieter | Umgebungsvariable |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter-Profile können profilspezifische Schlüsseleinstellungen verwenden, die über die GUI verwaltet werden.

### 2. Routenmodelle konfigurieren

Öffnen Sie die Einstellungen und wählen Sie das Upstream-Modell für jede Route aus:

- Opus
- Sonnet
- Haiku

Wählen oder erstellen Sie für OpenRouter zuerst ein Profil und konfigurieren Sie dann jede Route innerhalb dieses Profils.

### 3. Gateway starten

Klicken Sie auf **Gateway starten**.

Überprüfen Sie, ob der lokale Endpunkt verfügbar ist:

```text
GET http://127.0.0.1:4000/health
```

### 4. Claude Code über Anthro Bridge starten

Öffnen Sie das Claude-Konfigurationspanel und klicken Sie auf **Claude Code Startbefehl kopieren**.

Fügen Sie den generierten Befehl in PowerShell ein. Der Befehl enthält:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, wenn Kontextverwaltung angewendet wird
- `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, wenn Kontextverwaltung angewendet wird
- Bereinigungsbefehle für veraltete Kontextvariablen, wenn keine Kontextverwaltung angewendet wird

Der Befehl startet Claude Code mit Anthro Bridge als Gateway, während das konfigurierte modellbewusste Kontextverhalten erhalten bleibt.

Anleitungen für Claude Desktop und weitere Drittanbieter-Inferenz finden Sie in:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API-Endpunkte

| Methode | Pfad | Beschreibung |
|---|---|---|
| `GET` | `/health` | Gateway-Health-Check |
| `GET` | `/v1/models` | Öffentliche Routen-Modellliste |
| `POST` | `/v1/messages` | Streaming- und Nicht-Streaming-Messages-API |
| `POST` | `/v1/messages/count_tokens` | Token-Zählung, wenn vom ausgewählten Anbieter unterstützt |

## Konfiguration

Die Hauptkonfigurationsdatei ist `config.json`.

Die meisten Einstellungen sollten über die GUI geändert werden. Manuelle Bearbeitung ist für fortgeschrittene Anwender vorgesehen.

Wichtige Modellfelder umfassen:

| Schlüssel | Beschreibung |
|---|---|
| `models.<route>.upstream_model` | Upstream-Modellname, der an den Anbieter gesendet wird |
| `models.<route>.thinking_mode` | Routen-spezifischer Thinking-Modus |
| `models.<route>.reasoning_effort` | Anbieterspezifischer Reasoning-Aufwand |
| `models.<route>.supports_vision` | Bildunterstützungs-Override |
| `models.<route>.supports_video` | Videounterstützungs-Override |
| `models.<route>.visible` | Ob die Route für Clients und das Dashboard sichtbar ist |
| `non_vision_image_policy` | Wie nicht unterstützte Bildeingaben behandelt werden |
| `normalize_response_model_identity` | Ob Antwort-Modellnamen normalisiert werden |
| `claude_code.auto_compact.enabled` | Globaler Umschalter für die Kontextverwaltung |
| `claude_code.auto_compact.trigger_percent` | Angeforderter Prozentsatz für proaktive Komprimierung |
| `claude_code.auto_compact.mode` | `auto`, `manual` oder `claude_default` |
| `claude_code.auto_compact.window_tokens` | Manuelles Kontextfenster, das im Modus `manual` verwendet wird |

Nicht unterstützte Bilder können durch eine der folgenden Richtlinien behandelt werden:

- `replace`: das Bild durch einen Textplatzhalter ersetzen
- `drop`: den Bildinhalt entfernen
- `reject`: einen Fehler zurückgeben

### Konfiguration der Kontextverwaltung

Die GUI legt nur den globalen Umschalter für die Kontextverwaltung offen. Erweiterte Werte können direkt in `config.json` bearbeitet werden.

Automatischer Modus:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "auto",
      "trigger_percent": 90
    }
  }
}
```

Manueller Modus:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "manual",
      "window_tokens": 240000,
      "trigger_percent": 90
    }
  }
}
```

Standardverhalten von Claude Code:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "claude_default"
    }
  }
}
```

Im Modus `auto` wendet Anthro Bridge Kontextvariablen nur an, wenn alle drei kanonischen Routen bekannte Kontextmetadaten besitzen. Unbekannte benutzerdefinierte OpenRouter-Modelle bleiben gültige Routing-Ziele, aber die Kontextverwaltung meldet einen unvollständigen Zustand, bis Metadaten verfügbar sind oder der manuelle Modus konfiguriert wird.

Statische Modellkapazitäten werden gespeichert in:

```text
gui/src-tauri/resources/model_context_windows.json
```

Die Registrierung umfasst Standardmodelle von DeepSeek, MiniMax, Kimi, MiMo, Poolside, Tencent, InclusionAI, StepFun und OpenAI GPT-5.6, die von den integrierten Presets verwendet werden.

## Anbieterhinweise

### DeepSeek

`reasoning_effort`:

- `deepseek-v4-pro` (V4-Pro-0813)
  - Normal: Reasoning-Aufwand deaktiviert
  - Thinking: Low / High / Max
- `deepseek-v4-flash` (V4-Flash-0731)
  - Normal: Reasoning-Aufwand deaktiviert
  - Thinking: Low / High / Max

Beim Start wird ein für eine DeepSeek-V4-Pro-Route gespeicherter vorheriger `medium`- oder `xhigh`-Aufwand zu `high` migriert (entsprechend den effektiven Reasoning-Stufen von DeepSeek). Der Proxy normalisiert die Aufwandswerte auch vor dem Senden (`medium`/`xhigh` → `high`) über das Format `output_config.effort`.

Standardmäßiges DeepSeek-Routing für Neuinstallationen und neu generierte Konfigurationen:

- Opus 5 → V4 Flash, Thinking, Max
- Sonnet 5 → V4 Flash, Thinking, High
- Haiku 4.5 → V4 Flash, Thinking, Low

Bestehendes gespeichertes Routing wird nicht automatisch geändert.

### MiniMax

Das Verhalten von MiniMax-Modellen unterscheidet sich je nach Modellgeneration. Anthro Bridge wendet das vom ausgewählten Modell geforderte Anfrageformat an, einschließlich adaptivem oder deaktiviertem Thinking, sofern unterstützt.

### Kimi

Kimi-Modelle können je nach Modellfamilie entweder einen Thinking-Parameter oder einen festen Reasoning-Aufwand-Modus verwenden. Anthro Bridge übersetzt die GUI-Auswahl in das entsprechende Upstream-Anfrageformat.

### MiMo

MiMo verwendet `thinking_mode` anstelle des generischen `thinking`-Feldes für unterstützte Routen.

Die Vision-Unterstützung variiert je nach Modell. Anthro Bridge wendet die konfigurierte Richtlinie für nicht unterstützte Bilder an, wenn eine Route keine Bildeingabe akzeptieren kann.

### OpenRouter

OpenRouter-Modelle werden nach Anbieter gruppiert, sofern sie erkannt werden. Die GUI bietet:

- Modellsuche
- Anbietergruppierung
- Benutzerdefinierte Modelleingabe
- Fähigkeitsabzeichen
- Preisanzeige
- Modellbezogene Reasoning-Steuerung
- Einheitliche Aktualisierung der Modellliste

OpenRouter-Modellfähigkeiten und -verhalten können sich im Laufe der Zeit ändern. Live-Metadaten werden verwendet, wo verfügbar, während die integrierte Registrierung stabile Standardwerte für bekannte Modelle bereitstellt.

Das integrierte OpenAI-GPT-5.6-Balanced-Profil verwendet bei Neuinstallationen und neu generierten Konfigurationen standardmäßig Thinking High auf allen Routen:

- Opus 5 → GPT-5.6 Sol, Thinking, High
- Sonnet 5 → GPT-5.6 Terra, Thinking, High
- Haiku 4.5 → GPT-5.6 Luna, Thinking, High

Bestehendes gespeichertes Routing wird nicht automatisch geändert.

## Benutzeroberfläche

Die Einstellungsoberfläche umfasst:

- Einklappbare Anbieterabschnitte
- Opus-, Sonnet- und Haiku-Routenkonfiguration
- Modellsuche und Anbietergruppierung für OpenRouter
- Thinking- und Reasoning-Steuerung basierend auf den Modellfähigkeiten
- Benutzerdefinierte Upstream-Modelleingabe
- Automatisches Speichern von Routen
- Explizites Speichern des API-Schlüssels
- Speicherfortschritt und Fehlermeldungen
- Informationen zu Modellpreisen und -fähigkeiten
- Umschalter für Antwortmodell-Normalisierung
- Umschalter für die Claude-Code-Kontextverwaltung in der Kopfzeile
- Aktion zum Kopieren des Claude-Code-Startbefehls im Claude-Konfigurationspanel

Das Dashboard umfasst:

- Anbieter- oder OpenRouter-Profilauswahl
- Gateway-Status
- Aktuelle Routenzuordnungen
- Fähigkeitsindikatoren
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
│   │   │   ├── model_routing.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   │       ├── config.json
│   │       └── model_context_windows.json
│   └── package.json
└── LICENSE
```

### Im Entwicklungsmodus ausführen

```bash
cd gui
npm install
npm run tauri dev
```

### Entwicklungsvariante erstellen

Verwenden Sie unter Windows einen einzelnen Rust-Build-Job, um gelegentliche Compiler-Abbrüche zu vermeiden:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Entwicklungs-Builds verwenden:

- Fenstertitel: `Anthro Bridge (DEV)`
- Port: `4000`
- Anwendungsidentität: `com.soheidon.anthro-bridge.dev`
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
cargo test
```

Die Verifizierung der Kontextverwaltung deckt Folgendes ab:

- Gemeinsame Auflösung von Route zu Upstream zwischen dem Proxy und dem Kontext-Resolver
- Vollständige Modell-Kontextmetadaten für integrierte Direktanbieter- und OpenRouter-Modelle
- Automatische Auswahl des Mindestfensters über die drei kanonischen Routen hinweg
- Angewendete, deaktivierte, unvollständige, manuelle und Claude-Standard-Modi
- Offizielle Namen der Claude-Code-Umgebungsvariablen
- Darstellung und Escaping von PowerShell-Befehlen
- Gateway-Verbindungsvariablen
- Injektion von Umgebungsvariablen in einen echten Windows-PowerShell-Unterprozess
- Entfernung veralteter Kontextvariablen, wenn keine Kontextverwaltung angewendet wird
- Kopieren des generierten Startbefehls im Frontend

Speziell für die OpenRouter-Routenauswahl:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

Die OpenRouter-Auswahltests decken Folgendes ab:

- Erfasste Routenidentität während Warteschlangen-Speichervorgängen
- Routenübergreifender Zurücksetzungsschutz
- Schutz vor veralteten Callbacks
- Verhalten bei Wiederholung der Aktualisierung
- Gateway-Neustart nach fehlgeschlagener Aktualisierung
- Überschreibung laufender Anfragen
- Generationsbasierte Zurücksetzungsunterdrückung

Ein dedizierter Multi-Save-Test für die Neustart-Aggregation kann hinzugefügt werden, um das folgende Verhalten abzusichern:

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## Manuelle Verifizierungs-Checkliste

Automatisierte Tests bilden nicht jede Tauri- und React-Timing-Bedingung ab. Überprüfen Sie vor der Veröffentlichung Folgendes im Entwicklungs-Build:

- Jedes OpenRouter-Profil zeigt die korrekten Hover-Details
- Die Modellauswahl wird nach einer Änderung nicht sichtbar zurückgesetzt
- Thinking- und Reasoning-Auswahlen bleiben nach dem Speichern stabil
- Einstellungen bleiben nach dem Schließen und erneuten Öffnen des Einstellungsbildschirms korrekt
- Einstellungen bleiben nach dem Neustart der Anwendung korrekt
- Das Wechseln von Profilen während eines Speichervorgangs beschädigt keines der Profile
- Ein fehlgeschlagener Speichervorgang setzt nur die Route zurück, die ihn ausgelöst hat
- Ein erfolgreicher Wiederholungsversuch der Aktualisierung löscht den vorherigen Fehler
- Ein fehlgeschlagener Wiederholungsversuch der Aktualisierung lässt den letzten Fehler sichtbar
- Der erforderliche Gateway-Neustart erfolgt einmal nach dem Stapel
- Benutzerdefinierte Modelle werden korrekt gespeichert und neu geladen
- Integrierte und Live-OpenRouter-Fähigkeiten werden korrekt angezeigt
- Der Umschalter für die Kontextverwaltung in der Kopfzeile verwendet einen visuellen Schalter und behält seinen Zustand bei
- Jedes integrierte Anbieter- oder OpenRouter-Preset löst alle drei Routenkapazitäten auf
- Der generierte Claude-Code-Befehl enthält Gateway-Verbindungsvariablen
- Bei aktivierter Kontextverwaltung enthält der generierte Befehl `CLAUDE_CODE_AUTO_COMPACT_WINDOW` und `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- Bei deaktivierter Kontextverwaltung entfernt der generierte Befehl beide Kontextvariablen
- Der kopierte Befehl startet Claude Code über das laufende Anthro-Bridge-Gateway

## Fehlerbehebung

### Port 4000 wird bereits verwendet

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Ein Modell lehnt Bild- oder Videoeingabe ab

Modellfähigkeiten variieren je nach Anbieter und Route. Überprüfen Sie die Fähigkeitsabzeichen in der GUI und wählen Sie eine kompatible Route.

Für nicht unterstützte Bildeingaben folgt Anthro Bridge der `non_vision_image_policy`.

### Einstellungen werden nach einem Upgrade zurückgesetzt

Starten Sie die Anwendung zuerst neu, damit Migrationen ausgeführt werden können.

Wenn das Problem weiterhin besteht:

1. Sichern Sie die Benutzerkonfiguration.
2. Vergleichen Sie sie mit der gebündelten Konfiguration.
3. Entfernen Sie veraltete Felder oder setzen Sie die Benutzerkonfiguration bei Bedarf zurück.

Speicherort der stabilen Konfiguration:

```text
%APPDATA%\Anthro Bridge\config.json
```

Speicherort der Entwicklungskonfiguration:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter-Modellliste ist veraltet

Verwenden Sie die einheitliche Modellaktualisierungssteuerung in den Einstellungen. Anthro Bridge speichert Modellmetadaten zwischen, sodass nach einer Änderung eines Modelleintrags durch OpenRouter möglicherweise eine manuelle Aktualisierung erforderlich ist.

### Kontextverwaltung ist unvollständig

Die automatische Kontextverwaltung erfordert bekannte Kapazitäten für alle drei kanonischen Routen.

Überprüfen Sie die konfigurierten Upstream-Modelle für Opus, Sonnet und Haiku. Ein benutzerdefiniertes oder neu veröffentlichtes Modell ist möglicherweise noch nicht in `model_context_windows.json` vorhanden.

Optionen:

1. Wählen Sie ein integriertes Modell mit bekannten Metadaten.
2. Fügen Sie verifizierte Modellmetadaten zur statischen Registrierung hinzu.
3. Verwenden Sie den manuellen Modus in `config.json`.
4. Verwenden Sie `claude_default`, um die Komprimierung vollständig Claude Code zu überlassen.

### Claude Code verwendet nicht die erwarteten Kontexteinstellungen

Bestätigen Sie, dass Claude Code aus dem generierten PowerShell-Befehl gestartet wurde und nicht aus einem separaten Terminalbefehl.

Überprüfen Sie in derselben PowerShell-Sitzung:

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

Diese Werte bestätigen, dass die Startumgebung vorbereitet wurde. Sie beweisen nicht, dass Claude Code die Variablen übernommen hat. Verwenden Sie für die endgültige Bestätigung Claude-Code-Diagnosen oder beobachten Sie das Komprimierungsverhalten.

## Übersetzung

Englisch ist die Quell-README.

Übersetzte README-Dateien werden unter `docs/` gespeichert. Wenn sich die englische README ändert, generieren oder aktualisieren Sie die übersetzten Dateien aus der englischen Quelle, anstatt jede Sprache unabhängig zu bearbeiten.

Sprachdateien für die Anwendungsoberfläche werden gespeichert unter:

```text
gui/src/i18n/lang/
```

## Lizenz

MIT-Lizenz. Siehe [LICENSE](../LICENSE).
