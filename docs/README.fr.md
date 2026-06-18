[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## Français

### Aperçu

Anthro Bridge est un outil de proxy + gestion GUI qui route les requêtes API de Claude Desktop / Claude Code vers les points de terminaison compatibles Anthropic de plusieurs fournisseurs.

Anthro Bridge lit le champ `model` de chaque requête et la route automatiquement vers le bon fournisseur upstream ( routage basé sur le modèle). Seul le champ `model` est réécrit — les messages, les blocs thinking, tool_use, tool_result et le SSE en flux transitent intacts.

Anthro Bridge n'est pas un fork, une GUI ou une application compagnon de Moon Bridge ; c'est une passerelle indépendante compatible Anthropic.

### Fournisseurs supportés

| ID du fournisseur | Nom d'affichage | Point de terminaison upstream | Modèle par défaut |
|-------------------|-----------------|-------------------------------|--------------------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

L'outil de gestion GUI (Tauri v2 + React 19 + TypeScript) fournit le contrôle démarrage/arrêt, l'édition de configuration, la visualisation des journaux et la gestion des clés API depuis une fenêtre Windows native.

### Pourquoi cette passerelle est nécessaire

Claude Desktop / Claude Code s'attend fondamentalement au format API et aux noms de modèles de la famille Claude d'Anthropic. Même lorsque des fournisseurs comme DeepSeek, MiniMax, Kimi et MiMo offrent des API compatibles Anthropic, Claude Desktop / Claude Code ne peut pas toujours les utiliser directement.

En particulier, **`inferenceModels[].name` de Claude Desktop n'accepte que les noms de modèles officiels Anthropic**. Les noms personnalisés de passerelle comme `claude-deepseek-v4` ou `kimi-k2.6` sont rejetés avec `"not an Anthropic model"`.

Pour contourner cette contrainte, Anthro Bridge **présente les noms de modèles officiels Anthropic (`claude-sonnet-4-6` / `claude-haiku-4-5`) comme "coquilles" à Claude Desktop, tandis que le LLM réel (DeepSeek / MiniMax / Kimi / MiMo) est sélectionné dans la GUI**.

```
Côté Claude Desktop (toujours fixe)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Passerelle interne (selon la sélection GUI)
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

Cela vous permet de passer la validation des noms de modèles de Claude Desktop tout en basculant librement entre DeepSeek, MiniMax, Kimi et MiMo.

### Prérequis

- **Windows 10/11** (locale japonaise supportée)
- Clé API du fournisseur choisi (DeepSeek / MiniMax / Kimi / MiMo — **un seul suffit**, depuis v0.5.0)

### Démarrage rapide

#### 1. Installation

Téléchargez le dernier installeur depuis [Releases](https://github.com/soheidon/anthro-bridge/releases) et exécutez-le.

L'installeur affiche un écran de sélection de langue au démarrage (choisissez parmi English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español).

#### Mise à jour

Exécutez simplement le nouveau `setup.exe` — il détecte et remplace automatiquement la version précédente. La désinstallation manuelle n'est pas nécessaire. Votre configuration (`%APPDATA%\Anthro Bridge\config.json`) est conservée lors des mises à jour.

#### 2. Configurer la clé API

Paramètres (⚙) → onglet **Clé API**, saisissez la clé API de votre fournisseur et cliquez sur **Enregistrer**.
La clé est persistée en tant que variable d'environnement utilisateur Windows.

| Fournisseur | Variable d'environnement | Notes |
|-------------|--------------------------|-------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. Sélectionner le fournisseur

Sur le tableau de bord, les tuiles des fournisseurs sont disposées en une grille 2×2 :

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

Cliquez sur une tuile pour sélectionner le fournisseur actif sous **Choisir le fournisseur LLM**.

#### 4. Démarrer la passerelle

Cliquez sur **Start Gateway** dans l'en-tête. Le proxy démarre sur `http://127.0.0.1:4000`.

#### 5. Configurer Claude Desktop / Cowork on 3P

Consultez [THIRD_PARTY_INFERENCE.fr.md](THIRD_PARTY_INFERENCE.fr.md) pour des instructions détaillées pas à pas.

### Points de terminaison

| Méthode | Chemin | Description |
|---------|--------|-------------|
| GET | `/health` | Vérification de santé |
| GET | `/v1/models` | Liste publique des modèles |
| POST | `/v1/messages` | Messages API (stream + non-stream). Routage basé sur le modèle |
| POST | `/v1/messages/count_tokens` | Comptage de tokens (fournisseurs supportés uniquement) |

### Routage

Routage basé sur le modèle : le champ `model` dans chaque requête détermine le fournisseur cible et le modèle upstream.

| Modèle Anthropic | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|------------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking activé) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking désactivé) | `mimo-v2.5` |

#### Détails du routage MiMo

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`** : Le Thinking est **activé par défaut** (`thinking_mode: "thinking"`). La clé `thinking_mode` (pas `thinking`) contrôle le comportement de thinking de MiMo. Réglez sur `"default"` pour le mode standard.
- **`claude-haiku-4-5` → `mimo-v2.5`** : Prend en charge le pass-through d'images (URL d'image et base64). L'entrée audio/vidéo n'est pas supportée par Anthro Bridge sur MiMo.
- **La route `claude-sonnet-4-6` ne supporte PAS les images.** Lorsque des images sont envoyées à cette route, elles sont remplacées par du texte de substitution (`non_vision_image_policy: "replace"`).
- **Point de terminaison upstream** : Les requêtes sont envoyées à `https://api.xiaomimimo.com/anthropic/v1/messages`.

### Langues

8 langues : English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español.

Pour ajouter une nouvelle traduction, déposez un fichier de langue (ex : `es.ts`) dans `gui/src/i18n/lang/` et reconstruisez.
Voir [CONTRIBUTING](CONTRIBUTING.md) pour les détails.

### Configuration (config.json)

Les paramètres du fournisseur définissent les noms de modèles upstream et les indicateurs de capacités par modèle. Aucune modification n'est généralement requise.
Les utilisateurs avancés peuvent modifier via Paramètres (⚙) → **Configuration de la passerelle**.

| Clé | Description |
|-----|-------------|
| `models.<model>.upstream_model` | Nom réel du modèle envoyé à l'upstream (requis) |
| `models.<model>.thinking` | Lorsqu'il est `"disabled"`, injecte la suppression du thinking (optionnel). Pour MiMo, utilisez `thinking_mode` à la place |
| `models.<model>.thinking_mode` | Spécifique à MiMo : `"thinking"` (activé) ou `"default"` (standard). Utilisé uniquement par le fournisseur MiMo |
| `models.<model>.supports_vision` | Support d'image par modèle (retourne à la valeur par défaut du fournisseur) |
| `models.<model>.supports_video` | Support vidéo par modèle (retourne à la valeur par défaut du fournisseur) |
| `models.<model>.visible` | S'il est affiché dans `/v1/models` et le tableau de bord (défaut `true`) |
| `non_vision_image_policy` | Traitement des images pour les modèles non-vision : `replace` (substitut) / `drop` / `reject` (erreur) |

### Structure du projet

```
anthro-bridge/
├── README.md
├── SPEC.md                    Spécification
├── docs/
│   ├── README.ja.md           Japonais
│   ├── README.zh-CN.md        Chinois simplifié
│   ├── README.zh-TW.md        Chinois traditionnel
│   ├── README.ko.md           Coréen
│   ├── README.fr.md           Français
│   ├── README.de.md           Allemand
│   ├── README.es.md           Espagnol
│   ├── THIRD_PARTY_INFERENCE.md   Guide d'inférence tiers
│   └── THIRD_PARTY_INFERENCE.*.md
├── LICENSE                    Licence MIT
├── config.json                Configuration du fournisseur
├── .gitignore
└── gui/
    ├── src/                   Frontend React (TypeScript)
    │   ├── components/        Composants UI
    │   ├── hooks/             Hooks personnalisés
    │   └── i18n/              Support multilingue
    │       └── lang/          Fichiers de langue (en, ja, zh-CN, zh-TW, ko, fr, de, es)
    ├── src-tauri/             Backend Tauri (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 commandes Tauri + cycle de vie du proxy
    │   │   ├── main.rs        Point d'entrée
    │   │   └── proxy.rs       Serveur proxy axum
    │   ├── resources/
    │   │   └── config.json    Configuration intégrée
    │   └── Cargo.toml
    └── package.json
```

### Build de développement

```bash
cd gui
npm install
npm run tauri build    # Build de production
npm run tauri dev      # Mode développement (HMR)
```

Nécessite la chaîne d'outils [Rust](https://rustup.rs/) stable et Node.js 24+.

### Dépannage

#### Port 4000 utilisé

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Image/vidéo rejetée

DeepSeek ne supporte pas les images ni les vidéos. Les images sont automatiquement remplacées par du texte de substitution (`non_vision_image_policy: "replace"`). Pour utiliser les images nativement, passez à MiniMax, Kimi ou MiMo (route `claude-haiku-4-5`).

La route `claude-sonnet-4-6` de MiMo ne supporte pas non plus les images — utilisez `claude-haiku-4-5` pour les tâches d'image. La vidéo est toujours rejetée.

#### MiMo : la configuration existante ne reflète pas les modifications

Si vous avez mis à jour depuis une version antérieure à v0.9.0, votre configuration utilisateur enregistrée peut encore avoir les anciennes valeurs `"display_name": "MiMo"` ou `"thinking": "default"`. v0.9.0 migre automatiquement ces valeurs au premier lancement, mais en cas de problème :

1. **Redémarrez l'application** — la migration automatique s'exécute au démarrage.
2. **Réinitialiser la configuration** : Supprimez `%APPDATA%\Anthro Bridge\config.json` et redémarrez. La configuration intégrée avec les paramètres MiMo corrects sera utilisée.
3. **Vérification manuelle** : Ouvrez `%APPDATA%\Anthro Bridge\config.json` et vérifiez que `providers.mimo` a `"display_name": "MiMo / Xiaomi"`, `"api_key_env": "XIAOMI_API_KEY"` et `thinking_mode` (pas `thinking`) sur les entrées de modèle.

### Test manuel — MiMo / Xiaomi

#### Texte uniquement (claude-sonnet-4-6 → mimo-v2.5-pro)

1. Définissez `XIAOMI_API_KEY` dans Paramètres → onglet Clé API → Enregistrer.
2. Sélectionnez **MiMo / Xiaomi** sur le tableau de bord.
3. Démarrez la passerelle.
4. Envoyez un message via Claude Desktop. Vérifiez que la réponse arrive avec des blocs thinking.

#### Test d'image (claude-haiku-4-5 → mimo-v2.5)

1. Sélectionnez **MiMo / Xiaomi** sur le tableau de bord.
2. Dans Claude Desktop, joignez une image à un message et envoyez-la.
3. Vérifiez que l'image est reçue et décrite correctement.
4. L'envoi d'une image à `claude-sonnet-4-6` doit entraîner un remplacement par du texte de substitution.

#### Vérification

Vérifiez le panneau de journal dans la GUI — les requêtes doivent montrer le champ `model` réécrit en `mimo-v2.5-pro` ou `mimo-v2.5` selon la route.

### Licence

MIT — voir [LICENSE](LICENSE) pour les détails.
