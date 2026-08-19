[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Retour au README Anthro Bridge](README.fr.md)

# Utiliser le MCP Anthro Bridge avec Google Antigravity

Anthro Bridge intègre un serveur Model Context Protocol (MCP) qui fournit un outil spécialisé `plan` (`anthro-bridge/plan`). Cela permet aux environnements d'agents tels que Google Antigravity de déléguer la conception architecturale et la planification d'implémentation à des modèles LLM externes (par exemple DeepSeek V4, MiMo, Kimi, MiniMax ou les modèles OpenRouter), tout en effectuant les modifications de code, les commandes de terminal, les compilations et les tests avec l'allocation de modèle incluse dans l'abonnement Antigravity.

---

## 1. Fonctionnement de ce flux de travail

```text
Antigravity
    ↓
Exploration du dépôt (inspection des fichiers et contexte)
    ↓
anthro-bridge / plan (appel MCP avec tâche, contexte, contraintes)
    ↓
Serveur MCP Anthro Bridge
    ↓
Modèle planificateur externe (configuré dans l'interface GUI)
    ↓
Plan d'implémentation structuré renvoyé
    ↓
Antigravity exécute les modifications,
la compilation et les tests via son abonnement
```

- **API externe** : Uniquement responsable de la génération du plan d'implémentation à partir du contexte du dépôt (facturée à l'usage par le fournisseur).
- **Abonnement Antigravity** : Prend en charge les boucles d'édition intensive, d'exécution d'outils et de tests.
- **Séparation des responsabilités** : Bénéficiez du raisonnement de pointe des modèles externes pour l'architecture sans gaspiller de jetons d'API externe dans la génération de code de routine.

---

## 2. Prérequis

1. **Anthro Bridge** installé sous Windows.
2. **`anthro-bridge-mcp-server.exe`** compilé ou disponible dans votre dossier d'installation (ex. : `mcp-server/target/release/anthro-bridge-mcp-server.exe`).
3. Une **clé API** configurée pour le fournisseur que vous souhaitez utiliser comme planificateur.
4. **Google Antigravity** installé et en cours d'exécution.

---

## 3. Configurer le serveur MCP dans Antigravity

1. Ouvrez Google Antigravity.
2. Accédez à :
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. Ajoutez la configuration du serveur `anthro-bridge` à l'objet `mcpServers` :

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> Vous n'avez pas besoin d'écrire vos clés API en clair dans le fichier de configuration MCP. Le serveur MCP lit automatiquement les variables d'environnement utilisateur Windows (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY`, etc.) ou la configuration enregistrée dans Anthro Bridge.

---

## 4. Vérifier la connexion MCP

Dans la vue **Installed MCP Servers** d'Antigravity, confirmez que `anthro-bridge` est bien reconnu :

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Configurer le modèle planificateur dans Anthro Bridge

1. Ouvrez l'application **Anthro Bridge**.
2. Sélectionnez l'onglet **MCP** en haut.
3. Choisissez le **Fournisseur (Provider)** ou le **Profil (Profile)** actif (DeepSeek, MiMo, OpenRouter, etc.).
4. Ouvrez les **Paramètres** (ou Paramètres détaillés du plan MCP) pour configurer :
   - **Modèle (Model)**
   - **Mode Thinking**
   - **Effort de raisonnement (Reasoning Effort)**
5. Enregistrez les paramètres.

> [!NOTE]
> Le serveur MCP d'Anthro Bridge recharge dynamiquement la configuration à chaque appel de l'outil `plan()`. Vous **n'avez pas** besoin de redémarrer le serveur MCP ou Antigravity lorsque vous modifiez le modèle dans l'interface GUI.

---

## 6. Utiliser manuellement l'outil plan

Vous pouvez demander directement à Antigravity d'invoquer le planificateur :

```text
Inspectez ce projet, puis utilisez l'outil MCP anthro-bridge/plan pour créer un plan d'implémentation. Ne commencez pas l'implémentation tout de suite.
```

Antigravity explorera les fichiers pertinents, résumera le contexte, appellera `anthro-bridge/plan` et vous présentera le plan obtenu pour révision.

---

## 7. Automatiser la planification avec une règle d'espace de travail

Créez un fichier de règle d'espace de travail dans [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) pour automatiser l'appel au planificateur :

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. Flux de travail automatisé typique

```text
Utilisateur : "Refactorisez la fonctionnalité X pour prendre en charge plusieurs profils."
    ↓
Antigravity inspecte les fichiers et résume le contexte
    ↓
Antigravity déclenche automatiquement l'appel à anthro-bridge/plan
    ↓
Anthro Bridge envoie la requête au modèle externe sélectionné
    ↓
Antigravity reçoit le plan d'implémentation structuré
    ↓
L'utilisateur examine et approuve le plan
    ↓
Antigravity applique les modifications et exécute les tests
```

---

## 9. Remarques importantes

- **Fonctionnement indépendant** : Le serveur MCP fonctionne indépendamment de la passerelle 3P Gateway. Il n'est pas nécessaire que la passerelle 3P soit active pour utiliser le MCP.
- **Facturation séparée** : Les appels à `anthro-bridge/plan` sont facturés par le fournisseur d'API externe concerné. Les modifications et tests ultérieurs utilisent le quota de votre abonnement Antigravity.
- **Prise en compte immédiate** : Les changements de paramètres dans l'interface GUI d'Anthro Bridge s'appliquent immédiatement dès le prochain appel à `plan()`.
