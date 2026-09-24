[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | [한국어](OLLAMA_LOCAL.ko.md) | Français | [Deutsch](OLLAMA_LOCAL.de.md) | [Español](OLLAMA_LOCAL.es.md)

[← Retour au README Anthro Bridge](../README.md)

# Claude Code + Ollama Local

Anthro Bridge peut acheminer Claude Code vers un modèle Ollama exécuté localement tout en conservant les fournisseurs cloud configurés pour Claude Desktop et MCP.

---

## Architecture

```text
Claude Code
     │
     │ Marqueur Claude Code Anthro Bridge (X-Anthro-Bridge-Client: claude-code)
     ▼
Passerelle Anthro Bridge
     │
     ├─ Route Gateway (`active_route = "gateway"`) ──→ Fournisseur cloud global (DeepSeek / MiMo / OpenRouter / etc.)
     │
     └─ Route Ollama  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                     Modèle local
```

Ollama Local est strictement réservé à **Claude Code**. Il ne remplace pas le fournisseur cloud global utilisé par Claude Desktop ou les outils MCP d'Anthro Bridge.

---

## Prérequis

1. **Anthro Bridge** installé et en cours d'exécution.
2. **Ollama** installé et actif sur votre machine locale.
3. Au moins un modèle installé dans Ollama (ex. `gemma4:latest`, `llama3.3:70b`, `qwen2.5-coder:32b`), ou un tag personnalisé valide.
4. Claude Code lancé à l'aide de la commande générée par Anthro Bridge.

---

## Configuration d'Ollama Local

Dans Anthro Bridge :
1. Allez dans **Paramètres > Clés API**.
2. Faites défiler jusqu'à la carte de configuration **Ollama Local** sous le tableau des clés API.

### Ligne de paramètres principale

- **Modèle** : Menu déroulant listant les modèles Ollama installés ainsi que les tags personnalisés.
- **Actualiser (`🔄 Actualiser`)** : Interroge le point de terminaison local `/api/tags` d'Ollama.
- **Thinking** : Choisissez **Normal (désactivé)** ou **Thinking (activé)**.
- **Afficher sur le tableau de bord** : Case à cocher pour afficher la carte Ollama Local sur le tableau de bord.

### Paramètres avancés (`▸ Paramètres avancés`)

- **Point de terminaison** : URL de base d'Ollama (par défaut : `http://127.0.0.1:11434`).
- **Vision (Base64)** : Active ou désactive l'envoi d'images pour les modèles locaux multimodaux.
- **Fenêtre de contexte** : Capacité optionnelle explicite en jetons (ex. `131072`).
- **Aucune clé API requise** : Les instances locales d'Ollama ne nécessitent pas de clé API.

---

## Découverte et actualisation des modèles

Cliquer sur **Actualiser (`🔄 Actualiser`)** envoie une requête :

```http
GET http://127.0.0.1:11434/api/tags
```

### Sécurité et préservation des configurations

- **Boucle locale uniquement** : Seules les adresses loopback (`127.0.0.1`, `localhost`, `[::1]`) sont autorisées.
- **Gestion d'erreur non bloquante** : Si Ollama n'est pas lancé ou expire (délai de 2 s), une notification apparaît sans effacer le modèle enregistré.
- **Préservation des modèles personnalisés** : Si un modèle configuré n'est pas dans la liste retournée, il reste sélectionné.
- **Tags personnalisés** : Sélectionnez `+ Tag de modèle personnalisé...` pour saisir manuellement n'importe quel identifiant.

---

## Sélectionner Ollama sur le tableau de bord

1. Activez **Afficher sur le tableau de bord** dans les paramètres.
2. Ouvrez le **Tableau de bord**.
3. Cliquez sur la carte **Ollama Local** :
   - Définit `claude_code.active_route = "ollama"`.
   - Le `active_provider` global (ex. `deepseek`) **reste inchangé**.
   - La carte Ollama est mise en surbrillance avec la mention **Actif pour Claude Code**.
4. Pour revenir au fournisseur cloud :
   - Cliquez sur n'importe quel fournisseur cloud sur le tableau de bord.
   - Claude Code repasse automatiquement à `claude_code.active_route = "gateway"`.

---

## Spécification du mode Thinking

Anthro Bridge convertit votre sélection de Thinking dans le format compatible Anthropic attendu par Ollama :

- **Normal** :
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking** :
  ```json
  "thinking": { "type": "enabled" }
  ```

---

## Fenêtre de contexte et compactage automatique

- `context_window` est **optionnel**.
- S'il est spécifié, Anthro Bridge l'utilise pour la gestion de contexte et le calcul d'auto-compactage de Claude Code.
- S'il est omis (`null`), Anthro Bridge n'invente pas de taille arbitraire.

---

## Marqueur d'identification Claude Code

La commande de lancement générée injecte un marqueur interne via `ANTHROPIC_CUSTOM_HEADERS` :

```text
X-Anthro-Bridge-Client: claude-code
```

- **Normalisation** : Insensible à la casse, supprime les doublons et préserve les en-têtes personnalisés de l'utilisateur.
- **Suppression avant transfert** : Utilisé uniquement en local par Anthro Bridge et supprimé avant transmission vers le fournisseur ou Ollama.

---

## Isolation des clients

| Client | Sélecteur de route | Cible |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` ou `"ollama"`) | Fournisseur du tableau de bord / Ollama Local |
| **Claude Desktop** | `active_provider` (Fournisseur cloud global) | Passerelle cloud (DeepSeek, MiMo, OpenRouter, etc.) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | Fournisseur dédié aux plans et revues MCP |

---

## Dépannage

### L'actualisation ne trouve pas Ollama
1. Vérifiez qu'Ollama est actif :
   ```powershell
   ollama list
   ```
2. Vérifiez que le point de terminaison est `http://127.0.0.1:11434`.

### Mon modèle enregistré n'apparaît pas
Anthro Bridge conserve le modèle enregistré même si `/api/tags` ne le renvoie pas. Vous pouvez aussi le saisir manuellement.

### Claude Code utilise toujours le fournisseur cloud
1. Vérifiez sur le tableau de bord que la carte **Ollama Local** est active.
2. Assurez-vous d'avoir lancé Claude Code avec la commande générée par Anthro Bridge.

### Claude Desktop continue d'utiliser le fournisseur cloud
C'est le comportement attendu. Ollama Local est exclusif à Claude Code.
