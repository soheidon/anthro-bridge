[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Utilisez Claude Code Desktop comme environnement de développement, routez l'implémentation vers des API tierces et utilisez des modèles externes comme planificateurs pour Antigravity.**

Anthro Bridge est une application compagnon Windows pour le développement assisté par IA, articulée autour de deux flux de travail principaux :

1. **Claude Code / Claude Desktop + Passerelle tierce (3P Gateway)** : Continuez à utiliser Claude Code Desktop comme environnement de développement d'agents tout en routant les requêtes via une passerelle locale compatible Anthropic vers des API LLM tierces (DeepSeek, MiMo, MiniMax, Kimi et OpenRouter).
2. **Antigravity + Planificateur MCP (MCP Planner)** : Déléguez la conception architecturale et la planification d'implémentation à des modèles externes via l'outil `plan` MCP d'Anthro Bridge (`anthro-bridge/plan`), tout en effectuant les modifications de code et les tests avec l'allocation de modèle incluse dans votre abonnement Antigravity.

---

## Deux flux de travail principaux

### 1. Claude Code / Claude Desktop avec 3P Gateway

Continuez à utiliser Claude Code Desktop et Claude Desktop comme environnement d'agent tout en routant les requêtes vers des API LLM tierces non prises en charge nativement par les clients Anthropic.

```text
Claude Code / Claude Desktop
             ↓
  Passerelle 3P Anthro Bridge
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **Séparation de l'environnement et du modèle** : Conservez l'exploration de dépôt, l'utilisation d'outils, l'édition de fichiers et l'exécution de tests de Claude tout en routant l'inférence vers des fournisseurs tiers.
- **Routage multi-profil dynamique** : Changez de fournisseur actif ou de profil OpenRouter instantanément depuis le tableau de bord GUI et personnalisez les routes Opus, Sonnet et Haiku dans les paramètres.
- **Guide de configuration** : [Guide de configuration 3P Gateway pour Claude Desktop](THIRD_PARTY_INFERENCE.fr.md)

### 2. Antigravity avec Planificateur MCP

Déléguez la planification et la conception architecturale à des modèles externes via l'outil `plan` MCP d'Anthro Bridge (`anthro-bridge/plan`), tout en exécutant les modifications de fichiers et les commandes de terminal avec l'allocation de modèle de votre abonnement Antigravity.

```text
Antigravity
    ↓
Exploration du dépôt (collecte de contexte)
    ↓
anthro-bridge / plan (MCP)
    ↓
Serveur MCP Anthro Bridge
    ↓
Modèle LLM externe configuré
    ↓
Plan d'implémentation structuré
    ↓
Antigravity exécute les modifications,
la compilation et les tests via l'abonnement
```

- **Répartition planification vs exécution** : Les modèles externes génèrent le plan global ; la capacité de l'abonnement Antigravity exécute les modifications de code et les boucles de test intensives en tokens.
- **Configuration GUI en direct** : La modification du fournisseur, du modèle ou de l'effort de raisonnement dans Anthro Bridge prend effet immédiatement lors du prochain appel de `plan()`, sans redémarrer Antigravity.
- **Guide de configuration** : [Guide de configuration Google Antigravity + MCP Anthro Bridge](ANTIGRAVITY_MCP.fr.md)

---

## Fournisseurs pris en charge

| Fournisseur | Type de connexion | Familles de modèles prises en charge | Contrôles de raisonnement |
|---|---|---|---|
| **DeepSeek** | API directe | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | API directe | MiniMax M3, M2.7 | Spécifique au modèle |
| **Kimi / Moonshot** | API directe | Kimi K2.x, Kimi K3 | Thinking / Effort de raisonnement |
| **MiMo / Xiaomi** | API directe | MiMo V2.5, V2.5 Pro | Mode Thinking |
| **OpenRouter** | Passerelle multi-profil | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini, etc. | Spécifique au modèle / profil |

---

## Installation

Téléchargez le dernier installateur Windows (`Anthro Bridge_x.x.x_x64-setup.exe`) depuis la page [Releases](https://github.com/soheidon/anthro-bridge/releases) et exécutez-le.

L'installateur prend en charge 8 langues (anglais, japonais, chinois simplifié, chinois traditionnel, coréen, français, allemand, espagnol) et conserve les paramètres utilisateur existants lors des mises à niveau.

---

## Démarrage rapide

### Flux 1 : Passerelle 3P pour Claude Code / Claude Desktop

1. Ouvrez **Paramètres > Clé API** dans Anthro Bridge et configurez une clé API pour le fournisseur souhaité.
2. Sélectionnez votre fournisseur ou votre profil OpenRouter sur le tableau de bord.
3. Cliquez sur **Démarrer la passerelle (Start Gateway)** (écoute sur `http://127.0.0.1:4000`).
4. Connectez Claude Code ou Claude Desktop :
   - **Claude Code** : Cliquez sur **Copier la commande de lancement de Claude Code** dans les paramètres et collez-la dans PowerShell.
   - **Claude Desktop / Cowork** : Suivez le [Guide de configuration 3P pour Claude Desktop](THIRD_PARTY_INFERENCE.fr.md).

### Flux 2 : Planificateur MCP pour Google Antigravity

1. Configurez une clé API pour le modèle de planificateur choisi dans Anthro Bridge.
2. Sélectionnez l'onglet **MCP** dans Anthro Bridge et configurez votre modèle dans **Paramètres > Paramètres détaillés du plan MCP**.
3. Enregistrez `anthro-bridge-mcp-server.exe` dans la configuration MCP d'Antigravity.
4. Appelez `anthro-bridge/plan` dans Antigravity (ou automatisez-le avec une règle d'espace de travail).
5. Suivez le [Guide complet de configuration MCP pour Antigravity](ANTIGRAVITY_MCP.fr.md).

---

## Documentation

- [Guide de configuration 3P Gateway pour Claude Desktop](THIRD_PARTY_INFERENCE.fr.md)
- [Guide de configuration Google Antigravity + MCP Anthro Bridge](ANTIGRAVITY_MCP.fr.md)
- [Référence de configuration (`config.json`)](CONFIGURATION.md)
- [Détails des fournisseurs et comportements des modèles](PROVIDERS.md)
- [Guide de développement et de vérification](DEVELOPMENT.md)

---

## Dépannage

### Le port 4000 est déjà utilisé
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Les paramètres reviennent à l'état initial après une mise à niveau
Redémarrez l'application pour que les migrations s'exécutent. La configuration est enregistrée dans `%APPDATA%\Anthro Bridge\config.json`.

### L'appel au planificateur MCP échoue
Assurez-vous qu'une clé API est configurée pour le fournisseur sélectionné sous l'onglet **MCP** d'Anthro Bridge ou dans vos variables d'environnement utilisateur Windows (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, etc.). La passerelle 3P n'a pas besoin d'être active pour utiliser le MCP.

---

## Licence

Licence MIT. Voir [LICENSE](../LICENSE).
