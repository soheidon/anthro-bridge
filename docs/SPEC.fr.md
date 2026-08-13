[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## Aperçu

Un outil léger de proxy + gestion GUI qui route les requêtes API de Claude Desktop / Claude Code vers les points de terminaison compatibles Anthropic de plusieurs fournisseurs.

### Architecture

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Intégré dans l'app Tauri (axum 0.7 + reqwest)
       |
       | Routage par champ model -> résout le bon fournisseur upstream
       | Réécrit uniquement le model en nom upstream
       | Injecte thinking disabled pour les variantes non-thinking
       | Vérification du support média par modèle
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### Principes de conception

- **Modèle coquille + sélection de fournisseur** : Claude Desktop voit toujours `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`. Le LLM réel est sélectionné dans la GUI (DeepSeek / MiniMax / Kimi / MiMo / OpenRouter). Le mappage de modèles du fournisseur actif est utilisé pour le routage.
- **Support OpenRouter** : Achemine vers le point de terminaison compatible Anthropic d'OpenRouter avec les valeurs par défaut Poolside Laguna S/XS. Des commandes de mode thinking dédiées (Max/On/Off) sont traduites au format `reasoning` d'OpenRouter au moment de la requête.
- **Seul le fournisseur actif a besoin d'une clé API** : Depuis v0.5.0, seuls les fournisseurs référencés par la table de routage sont vérifiés au démarrage. Les clés des fournisseurs inactifs ne sont pas requises.
- **Proxy léger** : Rien n'est modifié sauf le champ `model`. Le SSE est transféré octet par octet.
- **Transfert sans perte** : Les corps de messages, les appels d'outils, les blocs thinking transitent intacts.
- **GUI native Windows** : Tauri v2 + React 19 + TypeScript. Backend Rust, frontend Vite + React 19.
- **Zéro dépendance externe** : Proxy intégré dans le binaire Tauri depuis v0.3.0. Python non requis.
- **Multilingue** : 8 langues (en, ja, zh-CN, zh-TW, ko, fr, de, es). Ajoutez de nouvelles langues en déposant des fichiers dans `lang/`. Sélecteur de langue au premier lancement.
- **Effort de raisonnement** : DeepSeek V4 Pro (V4-Pro-0813) et V4 Flash (V4-Flash-0731) prennent tous deux en charge l'effort de raisonnement Low / High / Max en mode Thinking. L'effort de raisonnement est désactivé en mode Normal. Un effort hérité `medium`/`xhigh` enregistré pour une route V4 Pro est migré vers `high` au démarrage. Le proxy normalise les valeurs d'effort avant l'envoi à DeepSeek (`medium`/`xhigh` → `high`) via `output_config.effort`.
- **Détection des capacités** : Des indicateurs de capacité en direct (supports_image_url, supports_image_base64, supports_video_url, supports_video_base64) sont récupérés depuis l'API OpenRouter et persistés dans config.json.
- **Conscience de la tarification pic/creux** : Les plages horaires de pointe DeepSeek et OpenRouter sont affichées dans le fuseau horaire local.
- **Bascule thinking MiniMax-M3** : MiniMax-M3 prend en charge Thinking ON/OFF via l'API compatible Anthropic (`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`). Les modèles M2.x restent en thinking uniquement. Une migration au démarrage convertit l'ancien `thinking_only` → `thinking` pour les utilisateurs existants.
- **Normalisation de l'identité du modèle de réponse** : Réécrit les noms de modèles upstream dans les réponses API (streaming SSE et non-streaming) vers les noms de modèles officiels Anthropic. Contrôlée par `normalize_response_model_identity` dans config.json et un `AtomicBool` au moment de l'exécution. Commande d'enregistrement indépendante (`update_normalize_model_identity`) pour éviter toute contamination croisée avec les enregistrements de configuration serveur.
- **Journalisation structurée des communications** : `tracing` + `tracing-appender` écrit des journaux structurés dans `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`. Chaque requête reçoit un ID de corrélation issu d'un compteur `AtomicU64`. Les entrées de journal incluent le modèle de requête, le modèle de passerelle, le modèle upstream, le résultat de normalisation et les raisons de saut. Aucune donnée sensible (prompts, corps de requêtes, clés API) n'est journalisée.
- **Badge PEAK** : Badge rose codé en couleur dans le tableau de bord pour les modèles tarifés en période de pointe.
- **Affichage du décalage UTC** : Le sélecteur de fuseau horaire affiche les décalages UTC dynamiques (par ex. UTC+09:00) à côté de chaque option.
- **Détection de l'échec de plafond de jetons Laguna S/XS 2.1** : Détecte les réponses de raisonnement uniquement avec `stop_reason: "max_tokens"` dans les flux SSE et les réponses non-stream. Journalise un avertissement lorsque la limite de jetons par tour est atteinte sans produire de texte ou d'appels d'outils utilisables. Disponible pour tous les modèles Poolside Laguna via OpenRouter.
- **Transmission thinking:disabled de Poolside** : Traduit `thinking: { type: "disabled" }` envoyé par le client au format `reasoning: { enabled: false }` d'OpenRouter pour les modèles Poolside, garantissant que le thinking désactivé est correctement transmis même sans paramètre de configuration enregistré.
- **Migration du défaut Laguna Opus** : Une migration idempotente unique change la valeur par défaut de `claude-opus-5` de thinking activé vers le mode normal pour les utilisateurs OpenRouter `poolside/laguna-s-2.1`. Le modèle de nouvelle installation reflète la valeur par défaut mise à jour.
- **Multi-profil OpenRouter** : Plusieurs profils OpenRouter par utilisateur, chacun avec sa propre clé API et sa configuration de modèles. CRUD des profils via les commandes Tauri. Bascule du profil actif depuis le tableau de bord ou les paramètres. Les profils peuvent être réordonnés par glisser-déposer, masqués et persistés dans l'ordre configuré.
- **Tuiles de tableau de bord OpenRouter** : Le tableau de bord crée une tuile par profil OpenRouter visible, avec une tuile de secours en l'absence de profils. Les résumés de modèles masquent l'espace de noms du fournisseur avant le premier `/` uniquement pour l'affichage OpenRouter ; les ID upstream complets restent inchangés pour le routage.
- **Registre de modèles OpenRouter** : Registre local intégré des modèles OpenRouter connus (`model_capabilities.rs`, `builtinOpenRouter.ts`) avec des capacités préconfigurées (vision, vidéo, politique de thinking, effort de raisonnement), un regroupement par fournisseur et des données de tarification. Utilisé pour la classification des modèles sans appels API en direct.
- **Détails de tarification OpenRouter** : La tarification intégrée prend en charge les valeurs actuelles et standard révisées pour les taux d'entrée, de sortie et d'entrée en cache, y compris les variantes GPT-5.6 Sol, Terra, Luna et Pro. La GUI affiche les tarifs promotionnels et standard ensemble lorsque les deux sont disponibles.
- **Support du modèle GPT-5.6** : Les profils OpenRouter peuvent utiliser les variantes de modèles Sol, Terra et Luna, avec des contrôles de thinking sensibles aux capacités et des notes de tarification pour les tarifs long contexte le cas échéant. Le profil intégré OpenAI GPT-5.6 Balanced route Opus 5 → GPT-5.6 Sol, Sonnet 5 → GPT-5.6 Terra et Haiku 4.5 → GPT-5.6 Luna avec un effort de raisonnement Thinking High sur les trois routes pour les nouvelles installations ; le routage enregistré existant n'est pas modifié automatiquement.
- **Dimensionnement de la fenêtre piloté par le tableau de bord** : Les changements initiaux et de nombre de lignes calculent la hauteur de la fenêtre à partir des tuiles visibles du tableau de bord dans une grille à trois colonnes. Le calcul tient compte de la hauteur des tuiles, des espaces de grille, de la taille minimale native, de la zone de travail du moniteur, de la mise à l'échelle DPI et des décorations de fenêtre tout en préservant le redimensionnement manuel lorsque le nombre de lignes est inchangé.
- **Programme d'installation NSIS localisé** : Le programme d'installation Windows propose des choix de langue anglais, japonais, chinois simplifié, chinois traditionnel, coréen, français, allemand et espagnol et intègre l'icône de l'application Anthro Bridge.
- **Couverture des régressions** : La couverture Vitest inclut l'ordre des profils OpenRouter et les courses d'enregistrement, les données de tarification de production, la sémantique du nombre de tuiles du tableau de bord et le dimensionnement de fenêtre tenant compte du moniteur.
- **Nouveaux fournisseurs via OpenRouter** : InclusionAI et StepFun ajoutés comme fournisseurs de modèles OpenRouter avec des indicateurs de capacité dédiés, des contrôles de mode thinking et un regroupement par fournisseur.
- **Modes thinking Tencent Hy3** : Prise en charge de l'effort de raisonnement Low/High pour le modèle Hunyuan de Tencent. La traduction du mode thinking dans proxy.rs mappe `thinking_mode` au format `reasoning` d'OpenRouter. L'interface affiche Low/High comme options de menu déroulant.
- **Correctifs Kimi K3** : Suppression du `forced_reasoning_effort` codé en dur des définitions de capacités. L'affichage fixe « Max » est remplacé par un sélecteur déroulant configurable. Valeurs par défaut issues de la configuration enregistrée, avec repli sur « max ».
- **Sérialisation des écritures de configuration** : Toutes les commandes Tauri d'écriture de configuration sont sérialisées via `execute_serialized_config_mutation` avec une protection `Mutex`. La structure `ConfigState` fournit le suivi `applied_config`, `in_flight_config` et `pending_ops` avec validation. Empêche les courses lors de l'enregistrement concurrent de plusieurs changements de paramètres.
- **Correctifs des courses d'interface OpenRouter** : (1) la ref de callback la plus récente `syncUiFromSavedRouteRef` empêche une fermeture obsolète d'écraser l'interface de la nouvelle route. (2) La protection `rollbackRouteId` empêche le rollback Phase 2 entre routes. (3) Le hook `useRouteSaveGeneration` fournit des protections de génération `begin()`/`isCurrent()` pour tous les gestionnaires. (4) Hook de file d'enregistrement (`useOpenRouterSaveQueue`) avec boucle de vidage, détection de remplacement et relance de l'agrégation OR.
- **Isolation de l'identité d'application dev/stable** : L'énumération `AppChannel` (`Stable`/`Dev`) dans `paths.rs` sélectionne un identifiant distinct (`com.soheidon.anthro-bridge` vs `.dev`), un répertoire de configuration distinct (`Anthro Bridge` vs `Anthro Bridge Dev`) et des chemins de cache distincts. Le canal Dev utilise `tauri.dev.conf.json`. Scripts NPM : `npm run dev` (dev), `npm run dev:stable` (stable).
- **Intégration du modèle de configuration** : `include_str!()` intègre `config_template.rs` au moment de la compilation, supprimant la dépendance d'exécution au `config.json` fourni. `merge_bundled_providers` renvoie un `Result` avec une gestion d'erreurs typée.
- **Tests de régression frontend** : 7 tests de régression vitest pour les courses d'enregistrement OpenRouter utilisant `QueueHarness` et `GenerationHandlerHarness`. Les tests couvrent : la ref de callback la plus récente, la protection de rollback entre routes, la capture d'identité, la nouvelle tentative d'actualisation (chemins échec + succès), le remplacement en cours et la protection de génération.
- **Gestion du contexte Claude Code** : Auto-compaction sensible au modèle pour Claude Code. `resolve_effective_auto_compact` résout chaque route standard (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) vers son modèle upstream, recherche la capacité de contexte de chaque modèle dans le registre statique `model_context_windows.json` et, en mode Auto, utilise la plus petite capacité connue comme fenêtre de contexte sûre. Le contrôle du contexte ne s'applique que lorsque les trois capacités sont connues (sinon, le statut est Incomplete). Une bascule d'en-tête active/désactive la gestion du contexte ; les modes avancés et les seuils sont définis dans `config.json` sous `claude_code.auto_compact`. Modes : `auto`, `manual` (`window_tokens`), `claude_default`.
- **Génération de la commande de lancement de Claude Code** : `build_claude_code_launch_command` génère une commande PowerShell complète combinant les variables de connexion à la passerelle (`ANTHROPIC_BASE_URL` pointant vers la passerelle locale, `ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`) avec les variables de contrôle du contexte de Claude Code (`CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`). Lorsque la gestion du contexte est désactivée, incomplète ou définie sur le défaut Claude, la commande supprime les variables de contexte obsolètes avec `Remove-Item Env:... -ErrorAction SilentlyContinue` afin que les valeurs de session définies précédemment ne fuient pas dans un nouveau lancement. Le bouton « Copier la commande de lancement de Claude Code » du panneau de configuration Claude copie la commande dans le presse-papiers. Anthro Bridge ne fait que générer et copier la commande — il ne l'exécute jamais.
- **Module de routage de modèles partagé** : `model_routing.rs` extrait la résolution route-à-upstream dans des fonctions pures partagées par `proxy.rs` et le résolveur de contexte, garantissant que les fenêtres de contexte résolvent les mêmes modèles upstream vers lesquels le proxy transfère réellement.
- **Registre des capacités de contexte** : `model_context_windows.json` est un registre statique des capacités de contexte connues couvrant les modèles intégrés des fournisseurs directs (DeepSeek, MiniMax, Kimi, MiMo) et les modèles OpenRouter intégrés (Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6). Les modèles OpenRouter personnalisés inconnus restent des cibles de routage valides mais rapportent la gestion du contexte comme Incomplete jusqu'à ce que des métadonnées soient ajoutées ou qu'un mode manuel soit configuré.

### Outil de gestion GUI

Tauri v2 + React 19 + TypeScript. Disposition à deux panneaux : Tableau de bord + Paramètres.

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [Démarrer/Arrêter passerelle] [État] [=]|
+------------------------------------------+
|  Tableau de bord                         |
|  +- Choisir le fournisseur LLM --------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]  ||
|  +- État ------------------------------+
|  | Port 4000 | Clé API | URL passerelle ||
|  | Table de routage des modèles         ||
|  +- Dernier journal ---------------------+
|  | Visionneuse avec compteurs Pro/Flash  ||
|  +---------------------------------------+
+------------------------------------------+

Paramètres (=):
  +- Langue -----------------------------+
  | Menu déroulant pour changement immédiat|
  +- Clé API ----------------------------+
  | Gestion de clé API par fournisseur    |
  +- Configuration Claude Desktop --------+
  | Génération JSON de config, copie,     |
  | détection de fichier de config        |
  +- Configuration de la passerelle ------+
  | Éditeur config.json (avancé)          |
  +---------------------------------------+
```

### Commandes Tauri

| # | Commande | Type | Description |
|---|----------|------|-------------|
| 1 | `check_health` | async | Vérification de santé du proxy |
| 2 | `check_gateway_status` | sync | Port 4000 + vivacité de la tâche tokio |
| 3 | `check_api_key` | sync | État de la clé API du fournisseur actif |
| 4 | `set_env_api_key` | sync | Persister la clé API via setx |
| 5 | `get_port_4000_process` | sync | Obtenir le PID du port 4000 via netstat |
| 6 | `read_config` | sync | Lire config.json |
| 7 | `read_config_raw` | sync | Texte brut config.json + détection d'encodage |
| 8 | `write_config` | sync | Enregistrer config.json (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | Lire le dernier journal |
| 10 | `read_log` | sync | Lire le fichier journal spécifié |
| 11 | `list_logs` | sync | Lister les fichiers journaux |
| 12 | `create_new_log` | sync | Créer un nouveau fichier journal |
| 13 | `open_logs_folder` | sync | Ouvrir le dossier des journaux |
| 14 | `open_path` | sync | Ouvrir un chemin arbitraire |
| 15 | `find_claude_configs` | sync | Détecter automatiquement les fichiers de configuration Claude Desktop |
| 16 | `start_proxy` | sync | Démarrer le proxy (résoudre config -> lancer -> vérifier le port) |
| 17 | `stop_proxy` | sync | Arrêter le proxy (arrêt gracieux) |
| 18 | `proxy_status` | sync | Vérifier la vivacité de la tâche |
| 19 | `check_all_api_keys` | sync | État des clés API de tous les fournisseurs |
| 20 | `update_active_provider` | sync | Enregistrer active_provider |
| 21 | `update_provider_api_key_env` | sync | Enregistrer provider api_key_env |
| 22 | `get_user_language` | sync | Obtenir la préférence de langue enregistrée |
| 23 | `set_user_language` | sync | Enregistrer la préférence de langue |
| 24 | `is_first_run` | sync | Déterminer le premier lancement (existence de user_prefs.json) |
| 25 | `openrouter_get_models` | async | Récupérer/mettre en cache le catalogue de modèles OpenRouter |
| 26 | `set_model_upstream` | sync | Enregistrer le modèle upstream + la configuration thinking + les indicateurs de capacité pour un modèle de passerelle |
| 27 | `update_server_config` | sync | Enregistrer les paramètres hôte/port/CORS du serveur |
| 28 | `update_normalize_model_identity` | sync | Enregistrer la bascule de normalisation de l'identité du modèle de réponse (met à jour la configuration + l'AtomicBool d'exécution) |
| 29 | `update_claude_code_auto_compact_global` | sync | Basculer la gestion du contexte globale de Claude Code (activée + pourcentage de déclenchement) |
| 30 | `update_claude_code_auto_compact_target` | sync | Définir le mode de contexte par fournisseur/profil (auto / manual / claude_default) + les jetons de fenêtre manuels |
| 31 | `update_claude_code_context_settings` | sync | Mise à jour atomique combinée des paramètres de contexte globaux + cibles |
| 32 | `resolve_claude_code_auto_compact` | sync | Résoudre les paramètres de contexte effectifs (mode, jetons de fenêtre, pourcentage de déclenchement, statut) |
| 33 | `build_claude_code_launch_command` | sync | Générer la commande PowerShell complète de lancement de Claude Code (variables d'environnement passerelle + contexte) |

### Serveur Proxy (proxy.rs)

Porté de Python vers Rust (axum 0.7/reqwest) en v0.3.0.

#### Points de terminaison

| Méthode | Chemin | Comportement |
|---------|--------|--------------|
| GET | `/health` | Vérification de santé |
| GET | `/v1/models` | Liste publique des modèles (uniquement `visible: true`) |
| POST | `/v1/messages` | Résolution de modèle -> injection thinking -> vérification média -> transfert (stream/non-stream) |
| POST | `/v1/messages/count_tokens` | Transférer à l'upstream si supporté |

#### Routage des modèles

Construit une table de recherche inverse de modèle de passerelle -> (fournisseur, modèle upstream) en utilisant la section `models` de chaque fournisseur. Comme tous les fournisseurs utilisent les mêmes noms de modèles de passerelle, `active_provider` gagne en cas de collision. Effectivement, seuls les modèles du fournisseur actif se retrouvent dans la table de routage.

#### Validation de la clé API (depuis v0.5.0)

Étape 1 : Construire la table de routage des modèles (aucune clé API requise)
Étape 2 : Vérifier uniquement les clés API des fournisseurs référencés par la table de routage

#### Injection de thinking

Pour les modèles avec `thinking: "disabled"` dans leur entrée de configuration, injecte `{"type": "disabled"}` uniquement lorsque l'utilisateur n'a pas explicitement défini le thinking.

#### Normalisation du modèle de réponse

Lorsque `normalize_response_model_identity` est activé, le proxy réécrit le champ `model` dans les réponses upstream :

- **Non-streaming** : Analyse la réponse JSON, réécrit `model` vers le nom canonique Anthropic, re-sérialise
- **Streaming (SSE)** : Intercepte les trames d'événement `message_start`, réécrit `model` sur place en utilisant le remplacement par plage d'octets pour préserver le formatage et les espaces SSE
- **Raisons de saut** : `disabled` (bascule désactivée), `non_success_status` (réponse non-200), `content_encoding_not_transformable` (gzip/brotli), `stream_error`, `stream_cancelled`
- **Logique de décision** : Fonctions pures (`should_normalize_nonstream`, `nonstream_skip_reason`) utilisées à la fois par le code de production et les tests

#### Vérification média / Sanitisation d'images

Les indicateurs `supports_vision` / `supports_video` par modèle déterminent le comportement. Pour les modèles non-vision recevant des images, `non_vision_image_policy` s'applique :
- `replace` (défaut) : Remplacer les blocs image par du texte de substitution
- `drop` : Supprimer les blocs image (insérer un substitut si le contenu devient vide)
- `reject` : Retourner une erreur 400

Les blocs vidéo retournent toujours 400. `non_vision_image_policy` est visible via `/health`.

#### Gestion du contexte Claude Code

Le contrôle du contexte de Claude Code utilise deux variables d'environnement officielles :

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

Pipeline du résolveur :

1. Résoudre chaque route standard (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) vers son modèle upstream
2. Rechercher la capacité de contexte de chaque modèle upstream dans `model_context_windows.json`
3. Exiger que les trois capacités soient connues
4. Utiliser la plus petite capacité connue comme fenêtre de contexte sûre
5. Appliquer le pourcentage de déclenchement configuré

Modes : `auto` (plus petite capacité connue), `manual` (`window_tokens`), `claude_default` (propre défaut de Claude Code ; aucune variable définie). Le statut effectif est `applied`, `disabled` ou `incomplete`.

La commande de lancement combine les variables de connexion à la passerelle avec les variables de contexte :

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

Lorsque le contrôle du contexte n'est pas appliqué, la commande supprime d'abord les variables obsolètes :

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

Le dépassement du pourcentage ne fait que déclencher la compaction plus tôt ; les valeurs qui retarderaient la compaction au-delà du défaut de Claude Code peuvent être ignorées. Anthro Bridge ne fait que générer et copier la commande — il ne l'exécute jamais, et cela ne prouve pas qu'une version spécifique de Claude Code honore ces variables (la confirmation finale nécessite des diagnostics de Claude Code ou un comportement de compaction observé).

### Multilingue

Architecture fichier-par-langue avec auto-découverte `import.meta.glob` :

```
gui/src/i18n/lang/
  en.ts      English (canonical — defines TranslationKey type)
  ja.ts      Japanese
  zh-CN.ts   Chinese Simplified
  zh-TW.ts   Chinese Traditional
  ko.ts      Korean
  fr.ts      French
  de.ts      German
  es.ts      Spanish
```

Pour ajouter une langue : copiez `en.ts`, traduisez, reconstruisez. Aucune modification de code requise.

### Référence config.json

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key env var name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "real-model-name" },
      "visible_models": ["claude-public-model-name"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "real-model-name",
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

Chaque fournisseur ou profil OpenRouter peut également définir un mode de contexte par défaut via `claude_code: { "auto_compact": { "mode": "auto" } }`. Le mode effectif pour une route est la valeur du fournisseur/profil, avec repli sur le bloc global ; `resolve_claude_code_auto_compact` renvoie le résultat résolu.
