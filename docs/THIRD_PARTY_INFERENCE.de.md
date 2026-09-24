[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# Anthro Bridge mit Claude Desktop / Cowork on 3P verwenden

Anthro Bridge kann als lokales Anthropic-kompatibles Gateway für
Claude Desktop / Cowork on 3P verwendet werden.

Claude Desktop / Cowork on 3P unterstützt Drittanbieter-Inferenz über
das integrierte Konfigurationsfenster.

Offizielle Dokumentation:

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Anthro Bridge starten

Starten Sie Anthro Bridge und stellen Sie sicher, dass das Gateway läuft.

Standardmäßig lauscht Anthro Bridge auf:

```text
http://127.0.0.1:4000
```

Lassen Sie Anthro Bridge während der Nutzung von Claude Desktop / Cowork on 3P geöffnet.

## 2. Entwicklermodus in Claude Desktop aktivieren

Öffnen Sie Claude Desktop.

Öffnen Sie auf Windows das Anwendungsmenü oben links.

Wählen Sie dann:

```text
Help → Troubleshooting → Enable Developer Mode
```

Nach der Aktivierung des Entwicklermodus erscheint ein neues `Developer`-Menü.

## 3. Drittanbieter-Inferenz-Einstellungen öffnen

Öffnen Sie:

```text
Developer → Configure third-party inference
```

Dies öffnet das Konfigurationsfenster für Drittanbieter-Inferenz.

## 4. Verbindung konfigurieren

Wählen Sie im Abschnitt `Connection`:

```text
Gateway
```

Geben Sie dann die folgenden Werte ein.

| Feld                  | Wert                                       |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | Leer lassen, sofern keine benutzerdefinierten Header benötigt werden |

Der `Gateway API key` muss mit dem in Anthro Bridge konfigurierten lokalen API-Schlüssel übereinstimmen.

## 5. Identität und Modelle konfigurieren

Fügen Sie im Abschnitt `Identity & Models` die Modell-IDs hinzu, die Claude Desktop in der Modellauswahl anzeigen soll.

Beispiel:

```text
claude-sonnet-4-6
claude-haiku-4-5
```

Sie können jedem Modell auch ein Anzeigekennzeichen geben.

Beispiel:

| Modell-ID            | Anzeigekennzeichen |
| -------------------- | ------------------ |
| `claude-sonnet-4-6`  | `Gateway Pro`      |
| `claude-haiku-4-5`   | `Gateway Flash`    |

Das erste Modell in der Liste wird als Standard-Eintrag verwendet.

Für jedes Modell: Klappen Sie die Zeile auf und bestätigen Sie, dass die `Model ID` genau der Modellname ist, den Claude Desktop an Anthro Bridge senden soll.

Aktivieren Sie `Offer 1M-context variant` nur, wenn Ihr Upstream-Anbieter und das ausgewählte Modell das erweiterte Kontextfenster tatsächlich unterstützen.

## 6. Beispielkonfiguration

Die obigen Einstellungen entsprechen der folgenden Drittanbieter-Inferenz-Konfiguration:

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-sonnet-4-6",
      "labelOverride": "Gateway Pro"
    },
    {
      "name": "claude-haiku-4-5",
      "labelOverride": "Gateway Flash"
    }
  ]
}
```

## 7. Anwenden und Claude Desktop neu starten

Wenden Sie die Gateway- und Modelllisteneinstellungen lokal an.

Starten Sie Claude Desktop bei Aufforderung neu.

Nach dem Neustart von Claude Desktop werden Anfragen von Cowork on 3P an Anthro Bridge gesendet. Anthro Bridge leitet die Anfragen dann an den in Anthro Bridge konfigurierten Upstream-Anbieter weiter.

## Xiaomi MiMo-Anbieter

Wenn Anthro Bridge für die Verwendung des Direct MiMo-Anbieters konfiguriert ist, werden Inferenzanfragen von Claude Desktop an Xiaomis MiMo-API weitergeleitet.

### MiMo-V2.6-Modelle

| Model ID | Anzeigename | Multimodal |
| --- | --- | --- |
| `mimo-v2.6-flash` | MiMo-V2.6-Flash | Image + Video |
| `mimo-v2.6-pro` | MiMo-V2.6-Pro | Image + Video |
| `mimo-v2.6-pro-ultraspeed` | MiMo-V2.6-Pro-UltraSpeed | Image + Video |

Alle V2.6-Modelle unterstützen ein Kontextfenster von 1.000.000 Token.

### Normal / Thinking

MiMo verwendet einen einfachen Normal/Thinking-Schalter. Es gibt keine Reasoning-Aufwandsstufen (Low/Medium/High).

| Einstellung | Upstream-Parameter | Verhalten |
| --- | --- | --- |
| Normal | `thinking: {"type": "disabled"}` | Standardinferenz |
| Thinking | `thinking: {"type": "enabled"}` | Erweitertes Denken |

### Standardrouten

Die eingebauten Routen des Direct MiMo-Presets:

| Gateway-Modell | Upstream-Modell | Thinking |
| --- | --- | --- |
| `claude-opus-5` | `mimo-v2.6-pro` | Thinking |
| `claude-sonnet-5` | `mimo-v2.6-pro` | Normal |
| `claude-haiku-4-5` | `mimo-v2.6-flash` | Thinking |

### V2.5-Abwärtskompatibilität

Gespeicherte `mimo-v2.5`-, `mimo-v2.5-pro`- und `mimo-v2.5-pro-ultraspeed`-Konfigurationen bleiben erhalten und funktionieren weiterhin. Beim Start werden nur unveränderte Werksrouten automatisch auf V2.6 migriert. Angepasste Routen, Thinking-Modus-Einstellungen und benutzerdefinierte `default_model`-Werte werden niemals überschrieben.

> [!NOTE]
> **Claude Desktop / Cowork on 3P nutzt Cloud-Provider über das Gateway**
>
> Claude Desktop und Cowork on 3P senden Inferenzanfragen an das Anthro Bridge Gateway (`http://127.0.0.1:4000`), das sie an Ihren konfigurierten Cloud-Upstream-Provider weiterleitet (DeepSeek, MiMo, Kimi, MiniMax, OpenRouter).
>
> Das **Ollama Local**-Backend ist strikt **Claude Code CLI** vorbehalten und vollständig von Claude Desktop und MCP isoliert. Für die lokale LLM-Einrichtung mit Claude Code lesen Sie bitte den [Ollama Local Leitfaden](OLLAMA_LOCAL.de.md).

---

## Hinweise

Anthro Bridge ist ein inoffizielles Anthropic-kompatibles lokales Gateway.

Es ist nicht mit Anthropic, Moon Bridge oder einem beliebigen Upstream-Modellanbieter verbunden.

Claude Desktop / Cowork on 3P wird über die Drittanbieter-Inferenz-Einstellungen von Claude konfiguriert. Menübeschriftungen und Konfigurationsfelder können sich ändern, wenn Anthropic Claude Desktop aktualisiert.
