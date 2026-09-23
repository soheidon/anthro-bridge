[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Utilisez Claude Code / Claude Desktop comme environnement de développement, routez les inférences vers des API LLM tierces, et utilisez des modèles externes comme planificateurs et réviseurs pour Google Antigravity.**

Anthro Bridge est une application Windows complémentaire pour le développement logiciel assisté par IA. Elle prend en charge deux flux de travail complémentaires :

1. **Passerelle 3P pour Claude Code / Claude Desktop** — Conservez l'exploration de dépôts, l'utilisation d'outils, l'édition de fichiers et l'exécution de tests de Claude tout en routant les inférences vers des fournisseurs tiers.
2. **Planificateur & Réviseur MCP pour Google Antigravity** — Déléguez la planification d'implémentation et la révision post-implémentation à des modèles externes via les outils MCP `anthro-bridge/plan` et `anthro-bridge/review`.

---

## Deux flux de travail principaux

### 1. Claude Code / Claude Desktop avec la passerelle 3P

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **Séparation environnement & modèle** : Conservez les outils agentiques de Claude tout en routant les inférences vers des fournisseurs tiers.
- **Routage multi-profils dynamique** : Changez de fournisseur actif, de profil OpenRouter et de route de modèle depuis l'interface graphique.
- **Guide de configuration** : [Configuration de la passerelle 3P pour Claude Desktop / Cowork](THIRD_PARTY_INFERENCE.fr.md)

### 2. Antigravity avec le planificateur & réviseur MCP

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Modèle externe configuré (planificateur / réviseur)
    ↓
Plan d'implémentation / Verdict de révision
    ↓
Antigravity implémente et teste
en utilisant la capacité d'abonnement
```

- **Séparation planification / exécution** : Les modèles externes génèrent le plan de haut niveau ou le verdict de révision ; la capacité d'abonnement Antigravity exécute les modifications de code gourmandes en tokens.
- **Configuration GUI en temps réel** : Le changement de fournisseur, de modèle ou d'effort de raisonnement du planificateur ou du réviseur prend effet immédiatement à la prochaine invocation.
- **Guide de configuration** : [Configuration de Google Antigravity + Anthro Bridge MCP](ANTIGRAVITY_MCP.fr.md)

**Commandes globales Antigravity :**

- **`/anthro-plan`** — Délègue la planification d'implémentation au modèle externe configuré.
- **`/anthro-revise`** — Révise un plan existant en fonction de nouveaux retours ou contraintes.
- **`/anthro-review`** — Révise une implémentation terminée par rapport au plan approuvé avant le commit, avec des verdicts explicites READY / NOT READY.

**Flux de travail recommandé :**

```text
/anthro-plan → Implémentation & Tests → /anthro-review → Commit
```

---

## Fournisseurs pris en charge

| Fournisseur | Connexion | Familles prises en charge | Contrôles de raisonnement |
|---|---|---|---|
| **DeepSeek** | API directe | DeepSeek V4.1 Flash, V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | API directe | kimi-for-coding, kimi-for-coding-highspeed | Mode thinking |
| **MiniMax** | API directe | MiniMax M3, M2.7 | Spécifique au modèle |
| **Kimi / Moonshot** | API directe | Kimi K2.x, Kimi K3 | Thinking / Effort de raisonnement |
| **MiMo / Xiaomi** | API directe | MiMo V2.6 Flash, Pro, Pro-UltraSpeed (rétrocompatible V2.5) | Normal / Thinking |
| **OpenRouter** | Passerelle multi-profils | Voir la section OpenRouter ci-dessous | Spécifique au modèle / au profil |

### DeepSeek (Direct)

Le préréglage intégré **Direct DeepSeek** route : Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low.

- `deepseek-v4.1-flash` — Modèle phare actuel avec raisonnement ($0.27 / 1M tokens en entrée · $1.10 / 1M tokens en sortie).
- `deepseek-v4-pro-0813` — Référence haute qualité sans raisonnement étendu ($0.27 / 1M tokens en entrée · $1.10 / 1M tokens en sortie).

### Kimi Code (Direct)

API spécialisée dans le code (`KIMI_CODE_API_KEY`), distincte de Moonshot Kimi :

- `kimi-for-coding` — Modèle de codage pleine qualité.
- `kimi-for-coding-highspeed` — Variante à faible latence.

### MiMo / Xiaomi (Direct)

Préréglages intégrés **Direct MiMo** :
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- Modèle par défaut : `mimo-v2.6-flash`

Modèles : `mimo-v2.6-flash`, `mimo-v2.6-pro`, `mimo-v2.6-pro-ultraspeed` (sélectionnables). Les trois prennent en charge une fenêtre de contexte de 1 million de tokens et des capacités multimodales natives (texte, image, vidéo).

**Normal / Thinking** : MiMo utilise un simple commutateur Normal/Thinking — sans niveaux d'effort de raisonnement.

**Rétrocompatibilité V2.5** : Les routes `mimo-v2.5`, `mimo-v2.5-pro` et `mimo-v2.5-pro-ultraspeed` enregistrées sont préservées et continuent de fonctionner. Les anciens paramètres par défaut non modifiés migrent automatiquement vers V2.6 au démarrage.

### OpenRouter

Prend en charge plusieurs profils nommés. Catalogue complet de modèles OpenAI (liste déroulante unique) :

| ID du modèle | Nom d'affichage |
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

**GPT-6 Astra** : contexte de 1,05M · effort de raisonnement : `low / medium / high / xhigh / max`.  
**GPT-6 Astra Pro** : contexte de 1,05M · raisonnement Pro toujours activé (`reasoning.mode = pro`), effort non sélectionnable par l'utilisateur.  
**GPT Astra Latest** : alias suivant le dernier modèle de la famille Astra.

Préréglage intégré **OpenRouter: chatGPT** : Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium.

Également disponibles : **OpenRouter: Gemini** (Gemini 3.8 Flash · effort de raisonnement `low / medium / high`), **OpenRouter: Poolside**, **OpenRouter: Tencent**, **OpenRouter: InclusionAI**, **OpenRouter: StepFun**.

---

## Tarification des modèles (à partir de la v0.22.1)

| Modèle | Entrée | Sortie |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash (OpenRouter) | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## Installation

Téléchargez le dernier installateur Windows (`Anthro Bridge_x.x.x_x64-setup.exe`) depuis la page [Releases](https://github.com/soheidon/anthro-bridge/releases) et exécutez-le.

L'installateur prend en charge 8 langues et préserve les paramètres utilisateur existants lors des mises à niveau.

---

## Démarrage rapide

### Flux de travail 1 : Passerelle 3P pour Claude Code / Claude Desktop

1. Ouvrez Anthro Bridge **Paramètres > Clé API** et configurez une clé API pour le fournisseur souhaité.
2. Sélectionnez votre fournisseur ou profil OpenRouter sur le tableau de bord.
3. Cliquez sur **Start Gateway** (s'exécute sur `http://127.0.0.1:4000`).
4. Connectez Claude Code ou Claude Desktop :
   - **Claude Code** : Cliquez sur **Copy Claude Code launch command** dans les Paramètres et collez la commande dans PowerShell.
   - **Claude Desktop / Cowork** : Suivez le [Guide de configuration 3P pour Claude Desktop](THIRD_PARTY_INFERENCE.fr.md).

### Flux de travail 2 : Planificateur & Réviseur MCP pour Google Antigravity

1. Configurez une clé API pour le modèle planificateur/réviseur choisi dans Anthro Bridge.
2. Sélectionnez l'onglet **MCP** et configurez votre modèle dans **Paramètres > Antigravity > MCP Plan Settings**.
3. Enregistrez `anthro-bridge.exe` avec `["--mcp-server"]` dans la configuration MCP d'Antigravity (ou cliquez sur **Configure Automatically** dans Anthro Bridge).
4. Utilisez `/anthro-plan` pour concevoir des plans, `/anthro-revise` pour mettre à jour des plans, et `/anthro-review` pour réviser les implémentations avant le commit.
5. Suivez le [Guide complet de configuration du MCP Antigravity](ANTIGRAVITY_MCP.fr.md).

---

## Clés API

| Fournisseur | Variable d'environnement |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## Documentation

- [Configuration de la passerelle 3P pour Claude Desktop / Cowork](THIRD_PARTY_INFERENCE.fr.md)
- [Configuration de Google Antigravity + Anthro Bridge MCP](ANTIGRAVITY_MCP.fr.md)
- [Référence de configuration (`config.json`)](CONFIGURATION.md)
- [Détails des fournisseurs & contrôles de raisonnement](PROVIDERS.md)
- [Guide de développement & de vérification](DEVELOPMENT.md)

---

## Dépannage

### Le port 4000 est déjà utilisé
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Les paramètres reviennent à leur état précédent après une mise à niveau
Redémarrez l'application pour que les migrations puissent s'exécuter. La configuration est stockée dans `%APPDATA%\Anthro Bridge\config.json`.

### Les appels du planificateur MCP échouent
Assurez-vous qu'une clé API est définie pour le fournisseur sélectionné sous l'onglet **MCP**, ou exportée dans vos variables d'environnement utilisateur Windows (par exemple, `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`). La passerelle 3P n'a pas besoin d'être en cours d'exécution pour le MCP.

---

## Licence

Licence MIT. Voir [LICENSE](../LICENSE).
