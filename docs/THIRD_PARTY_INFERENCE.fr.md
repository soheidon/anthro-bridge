[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# Utiliser Anthro Bridge avec Claude Desktop / Cowork on 3P

Anthro Bridge peut être utilisé comme une passerelle locale compatible Anthropic pour
Claude Desktop / Cowork on 3P.

Claude Desktop / Cowork on 3P supporte l'inférence tierce via la
fenêtre de configuration intégrée.

Documentation officielle :

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Démarrer Anthro Bridge

Démarrez Anthro Bridge d'abord et assurez-vous que la passerelle est en cours d'exécution.

Par défaut, Anthro Bridge écoute sur :

```text
http://127.0.0.1:4000
```

Laissez Anthro Bridge en cours d'exécution pendant l'utilisation de Claude Desktop / Cowork on 3P.

## 2. Activer le mode développeur dans Claude Desktop

Ouvrez Claude Desktop.

Sous Windows, ouvrez le menu de l'application en haut à gauche.

Sélectionnez ensuite :

```text
Help → Troubleshooting → Enable Developer Mode
```

Après l'activation du mode développeur, un nouveau menu `Developer` apparaîtra.

## 3. Ouvrir les paramètres d'inférence tierce

Ouvrez :

```text
Developer → Configure third-party inference
```

Ceci ouvre la fenêtre de configuration de l'inférence tierce.

## 4. Configurer la connexion

Dans la section `Connection`, sélectionnez :

```text
Gateway
```

Entrez ensuite les valeurs suivantes.

| Champ                 | Valeur                                      |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | Laissez vide sauf si vous avez besoin d'en-têtes personnalisés |

La `Gateway API key` doit correspondre à la clé API locale configurée dans Anthro Bridge.

## 5. Configurer l'identité et les modèles

Dans la section `Identity & Models`, ajoutez les IDs de modèles que Claude Desktop doit afficher dans le sélecteur de modèles.

Exemple :

```text
claude-sonnet-4-6
claude-haiku-4-5
```

Vous pouvez également donner à chaque modèle un libellé d'affichage.

Exemple :

| ID du modèle         | Libellé d'affichage |
| -------------------- | ------------------- |
| `claude-sonnet-4-6`  | `Gateway Pro`       |
| `claude-haiku-4-5`   | `Gateway Flash`     |

Le premier modèle de la liste est utilisé comme entrée par défaut du sélecteur.

Pour chaque modèle, développez la ligne et confirmez que le `Model ID` est exactement le nom du modèle que vous souhaitez que Claude Desktop envoie à Anthro Bridge.

N'activez `Offer 1M-context variant` que si votre fournisseur upstream et le modèle sélectionné supportent réellement la fenêtre de contexte étendue.

## 6. Exemple de configuration

Les paramètres ci-dessus correspondent à la configuration d'inférence tierce suivante :

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

## 7. Appliquer et redémarrer Claude Desktop

Après avoir configuré la passerelle et la liste des modèles, appliquez les paramètres localement.

Redémarrez Claude Desktop si vous y êtes invité.

Une fois Claude Desktop redémarré, les requêtes de Cowork on 3P seront envoyées à Anthro Bridge. Anthro Bridge route ensuite les requêtes vers le fournisseur upstream configuré dans Anthro Bridge.

## Fournisseur Xiaomi MiMo

Lorsque Anthro Bridge est configuré pour utiliser le fournisseur Direct MiMo, les requêtes d'inférence de Claude Desktop sont acheminées vers l'API MiMo de Xiaomi.

### Modèles MiMo-V2.6

| Model ID | Nom d'affichage | Multimodal |
| --- | --- | --- |
| `mimo-v2.6-flash` | MiMo-V2.6-Flash | Image + Video |
| `mimo-v2.6-pro` | MiMo-V2.6-Pro | Image + Video |
| `mimo-v2.6-pro-ultraspeed` | MiMo-V2.6-Pro-UltraSpeed | Image + Video |

Tous les modèles V2.6 supportent une fenêtre de contexte de 1 000 000 tokens.

### Normal / Thinking

MiMo utilise un simple bouton bascule Normal/Thinking. Il n'y a pas de niveaux d'effort de raisonnement (Low/Medium/High).

| Paramètre | Paramètre upstream | Comportement |
| --- | --- | --- |
| Normal | `thinking: {"type": "disabled"}` | Inférence standard |
| Thinking | `thinking: {"type": "enabled"}` | Réflexion étendue |

### Routes par défaut

Les routes intégrées du preset Direct MiMo :

| Modèle passerelle | Modèle upstream | Thinking |
| --- | --- | --- |
| `claude-opus-5` | `mimo-v2.6-pro` | Thinking |
| `claude-sonnet-5` | `mimo-v2.6-pro` | Normal |
| `claude-haiku-4-5` | `mimo-v2.6-flash` | Thinking |

### Compatibilité ascendante V2.5

Les configurations `mimo-v2.5`, `mimo-v2.5-pro` et `mimo-v2.5-pro-ultraspeed` sauvegardées sont préservées et continuent de fonctionner. Seules les routes par défaut d'usine non modifiées sont automatiquement migrées vers V2.6 au démarrage. Les routes personnalisées, les paramètres de mode Thinking et les valeurs `default_model` personnalisées ne sont jamais écrasés.

> [!NOTE]
> **Claude Desktop / Cowork on 3P utilise les fournisseurs cloud via la passerelle**
>
> Claude Desktop et Cowork on 3P envoient des requêtes d\'inférence à la passerelle Anthro Bridge (`http://127.0.0.1:4000`), qui les achemine vers votre fournisseur cloud en amont configuré (DeepSeek, MiMo, Kimi, MiniMax, OpenRouter).
>
> Le backend **Ollama Local** est strictement réservé à **Claude Code CLI** et reste totalement isolé de Claude Desktop et du MCP. Pour la configuration du LLM local avec Claude Code, veuillez consulter le [Guide Ollama Local](OLLAMA_LOCAL.fr.md).

---

## Notes

Anthro Bridge est une passerelle locale non officielle compatible Anthropic.

Il n'est pas affilié à Anthropic, Moon Bridge ou à un quelconque fournisseur de modèles upstream.

Claude Desktop / Cowork on 3P est configuré via les paramètres d'inférence tierce de Claude. Les libellés de menu et les champs de configuration peuvent changer à mesure qu'Anthropic met à jour Claude Desktop.
