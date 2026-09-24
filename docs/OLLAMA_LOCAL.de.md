[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | [한국어](OLLAMA_LOCAL.ko.md) | [Français](OLLAMA_LOCAL.fr.md) | Deutsch | [Español](OLLAMA_LOCAL.es.md)

[← Zurück zur Anthro Bridge README](../README.md)

# Claude Code + Ollama Local

Anthro Bridge kann Claude Code an ein lokal ausgeführtes Ollama-Modell weiterleiten, während Claude Desktop und MCP unverändert bei ihren konfigurierten Cloud-Providern verbleiben.

---

## Architektur

```text
Claude Code
     │
     │ Anthro Bridge Claude Code-Marker (X-Anthro-Bridge-Client: claude-code)
     ▼
Anthro Bridge Gateway
     │
     ├─ Gateway-Route (`active_route = "gateway"`) ──→ Globaler Cloud-Provider (DeepSeek / MiMo / OpenRouter etc.)
     │
     └─ Ollama-Route  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                     Lokales Modell
```

Ollama Local ist ausschließlich für **Claude Code** vorgesehen. Es ersetzt nicht den globalen Cloud-Provider für Claude Desktop oder die Anthro Bridge MCP-Tools.

---

## Anforderungen

1. **Anthro Bridge** installiert und aktiv.
2. **Ollama** lokal installiert und ausgeführt.
3. Mindestens ein heruntergeladenes Modell in Ollama (z. B. `gemma4:latest`, `llama3.3:70b`, `qwen2.5-coder:32b`) oder ein gültiger benutzerdefinierter Modell-Tag.
4. Claude Code wurde mit dem von Anthro Bridge generierten Startbefehl gestartet.

---

## Ollama Local konfigurieren

Öffnen Sie Anthro Bridge:
1. Gehen Sie zu **Einstellungen > API-Schlüssel**.
2. Scrollen Sie zur Einstellungskarte **Ollama Local** unterhalb der API-Schlüsseltabelle.

### Haupt-Einstellungszeile

- **Modell**: Dropdown-Menü mit installierten Ollama-Modellen und benutzerdefinierter Eingabe.
- **Aktualisieren (`🔄 Aktualisieren`)**: Fragt den lokalen Endpunkt `/api/tags` von Ollama ab.
- **Thinking**: Wählen Sie **Normal (Deaktiviert)** oder **Thinking (Aktiviert)**.
- **Auf Dashboard anzeigen**: Steuert die Anzeige der Ollama Local-Kachel auf dem Dashboard.

### Erweiterte Einstellungen (`▸ Erweiterte Einstellungen`)

- **Endpunkt**: Basis-URL für Ollama (Standard: `http://127.0.0.1:11434`).
- **Vision (Base64)**: Bildunterstützung für multimodale Modelle aktivieren/deaktivieren.
- **Kontextfenster**: Optionale explizite Kontextkapazität in Token (z. B. `131072`).
- **Kein API-Schlüssel erforderlich**: Lokale Loopback-Instanzen von Ollama benötigen keinen API-Schlüssel.

---

## Modellsuche & Aktualisierung

Ein Klick auf **Aktualisieren (`🔄 Aktualisieren`)** sendet:

```http
GET http://127.0.0.1:11434/api/tags
```

### Sicherheits- und Beibehaltungsregeln

- **Nur Loopback**: Es werden ausschließlich Loopback-Adressen (`127.0.0.1`, `localhost`, `[::1]`) abgefragt.
- **Fehlertoleranz**: Ist Ollama nicht erreichbar oder läuft in ein Timeout (2 Sekunden), wird ein Hinweis angezeigt, ohne das gespeicherte Modell zu löschen.
- **Modell-Erhaltung**: Ein gespeichertes Modell bleibt auch dann ausgewählt, wenn es in der Liste fehlt.
- **Benutzerdefinierte Tags**: Über `+ Benutzerdefiniertes Modell...` können beliebige Tags manuell eingegeben werden.

---

## Ollama auf dem Dashboard auswählen

1. Aktivieren Sie **Auf Dashboard anzeigen** in den Einstellungen.
2. Wechseln Sie zum **Dashboard**.
3. Klicken Sie auf die Kachel **Ollama Local**:
   - Setzt `claude_code.active_route = "ollama"`.
   - Der globale `active_provider` (z. B. `deepseek`) **bleibt unverändert**.
   - Die Ollama-Kachel wird als **Aktiv für Claude Code** markiert.
4. Zurück zur Cloud-Route:
   - Klicken Sie auf eine beliebige Cloud-Provider-Kachel auf dem Dashboard.
   - Claude Code wird automatisch auf `claude_code.active_route = "gateway"` zurückgesetzt.

---

## Thinking-Semantik

Anthro Bridge übersetzt Ihre Auswahl in das von Ollama unterstützte Anthropic-kompatible Format:

- **Normal**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking**:
  ```json
  "thinking": { "type": "enabled" }
  ```

---

## Kontextfenster & Auto-Kompaktierung

- `context_window` ist **optional**.
- Wenn angegeben, wird es für die Kontextverwaltung und Auto-Compact-Berechnung von Claude Code verwendet.
- Wenn weggelassen (`null`), wird keine willkürliche Größe angenommen.

---

## Claude Code-Identifikationsmarker

Der Startbefehl injiziert einen internen Marker über `ANTHROPIC_CUSTOM_HEADERS`:

```text
X-Anthro-Bridge-Client: claude-code
```

- **Normalisierung**: Groß-/Kleinschreibung wird ignoriert, Duplikate bereinigt und benutzerdefinierte Header beibehalten.
- **Entfernung vor Weiterleitung**: Wird lokal für das Routing verwendet und vor dem Senden an Upstream-Server entfernt.

---

## Client-Isolation

| Client | Routen-Auswahl | Ziel |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` oder `"ollama"`) | Ausgewählter Provider / Ollama Local |
| **Claude Desktop** | `active_provider` (Globaler Cloud-Provider) | Cloud-Gateway (DeepSeek, MiMo, OpenRouter etc.) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | Dedizierter MCP-Planungs- und Review-Provider |

---

## Fehlerbehebung

### Aktualisieren findet Ollama nicht
1. Stellen Sie sicher, dass Ollama ausgeführt wird:
   ```powershell
   ollama list
   ```
2. Prüfen Sie, ob der Endpunkt `http://127.0.0.1:11434` lautet.

### Gespeichertes Modell wird nicht aufgeführt
Anthro Bridge behält das Modell bei. Sie können es auch manuell eingeben.

### Claude Code nutzt weiterhin den Cloud-Provider
1. Prüfen Sie auf dem Dashboard, ob **Ollama Local** aktiv ist.
2. Stellen Sie sicher, dass Claude Code mit dem generierten Befehl gestartet wurde.

### Claude Desktop nutzt weiterhin den Cloud-Provider
Dies ist das beabsichtigte Verhalten. Ollama Local gilt nur für Claude Code.
