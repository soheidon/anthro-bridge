[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

Anthro Bridge est une passerelle locale et un outil de configuration de bureau qui permet à Claude Desktop et Claude Code d'utiliser plusieurs fournisseurs LLM tiers via une API compatible Anthropic.

L'application se compose de :

- Un serveur proxy local écrit en Rust
- Une interface graphique native Windows construite avec Tauri 2, React et TypeScript
- Un routage basé sur les modèles, des noms de modèles Anthropic vers les modèles amont spécifiques au fournisseur
- Une configuration par route du modèle, du raisonnement et des capacités

Anthro Bridge est un projet indépendant. Il ne s'agit pas d'un fork, d'une interface ou d'une application compagnon de Moon Bridge.

## Modèles pris en charge

Anthro Bridge prend en charge deux catégories de modèles amont.

### Intégrations natives

Ces fournisseurs sont pris en charge via leurs propres API compatibles Anthropic. Aucun compte OpenRouter n'est requis.

| Fournisseur | Familles de modèles prises en charge | Connexion |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro et V4 Flash | API directe du fournisseur |
| MiniMax | Variantes MiniMax M3 et M2.7 | API directe du fournisseur |
| Kimi / Moonshot | Kimi K2.x et Kimi K3 | API directe du fournisseur |
| MiMo / Xiaomi | Variantes MiMo V2.5 et V2.5 Pro | API directe du fournisseur |

### Modèles pris en charge via OpenRouter

Ces modèles sont accessibles via un profil OpenRouter. Chaque profil possède sa propre clé API, ses mappages de routes et ses paramètres de raisonnement.

| Fournisseur ou famille de modèles | Support intégré | Contrôles de raisonnement |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Oui | Contrôles Thinking spécifiques au modèle |
| Tencent Hy3 | Oui | Effort de raisonnement Bas et Élevé |
| InclusionAI Ring | Oui | Contrôles Thinking et de raisonnement spécifiques au modèle |
| StepFun Step 3.5 / Step 3.7 | Oui | Bas, Moyen et Élevé lorsque pris en charge |
| Famille InclusionAI Ling | Oui | Contrôles Thinking spécifiques au modèle |
| OpenAI GPT-5.6 Sol / Terra / Luna | Oui | Contrôles Thinking et de raisonnement spécifiques au modèle |

D'autres modèles OpenRouter peuvent également être sélectionnés depuis la liste des modèles OpenRouter en direct ou saisis manuellement. Le support intégré signifie qu'Anthro Bridge connaît déjà la famille du modèle, les indicateurs de capacité, le regroupement par fournisseur et le comportement des contrôles de raisonnement.

## Fonctionnement

Claude Desktop et Claude Code envoient des requêtes en utilisant des noms de modèles Anthropic tels que :

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge traite ces noms comme des identifiants de route stables. L'interface graphique détermine quel fournisseur et quel modèle amont chaque route utilise.

Exemple :

```text
Requête Claude Code
  model: claude-sonnet-5

Route Anthro Bridge
  provider: Profil OpenRouter "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

Seuls les champs qui doivent être adaptés au fournisseur amont sont modifiés. Les messages, les appels d'outils, les résultats d'outils, les blocs de réflexion (thinking) et les données de streaming sont par ailleurs préservés lorsque l'API amont les prend en charge.

## Fonctionnalités principales

### Routage des fournisseurs

Anthro Bridge prend en charge deux types de connexion amont :

1. **Intégrations directes avec les fournisseurs**, qui se connectent à l'API compatible Anthropic propre d'un fournisseur.
2. **Profils OpenRouter**, qui se connectent à OpenRouter et peuvent router vers plusieurs fournisseurs et familles de modèles via une seule API.

#### Intégrations directes avec les fournisseurs

| ID du fournisseur | Nom d'affichage | Point de terminaison par défaut |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### Intégration OpenRouter

| Type de connexion | Nom d'affichage | Point de terminaison |
|---|---|---|
| Passerelle de modèles multi-profils | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter n'est pas traité comme un fournisseur de modèles unique. Chaque profil OpenRouter peut sélectionner indépendamment des modèles parmi les groupes de fournisseurs pris en charge tels que Poolside, Tencent, InclusionAI et StepFun, ainsi que d'autres modèles découverts depuis l'API OpenRouter ou saisis manuellement.

Chaque route Anthropic peut être mappée indépendamment soit vers un modèle de fournisseur direct, soit vers un modèle sélectionné via un profil OpenRouter.

### Support multi-profils OpenRouter

Plusieurs profils OpenRouter peuvent être créés et gérés indépendamment.

Chaque profil possède :

- Son propre nom de profil
- Sa propre configuration de clé API
- Ses propres mappages de routes Opus, Sonnet et Haiku
- Ses propres paramètres de réflexion (thinking) ou de raisonnement
- Sa propre liste de modèles OpenRouter mise en cache

Les profils peuvent être ajoutés, renommés, supprimés et sélectionnés depuis l'interface graphique.

Les groupes de fournisseurs OpenRouter intégrés incluent actuellement Poolside, Tencent, InclusionAI, StepFun et d'autres familles de modèles reconnues. Les modèles inconnus restent disponibles via la recherche ou la saisie personnalisée de modèle.

### Contrôles de modèle et de raisonnement

Les contrôles disponibles dépendent du modèle sélectionné.

Les contrôles pris en charge peuvent inclure :

- Thinking activé ou désactivé
- Modes de raisonnement Normal, Bas, Moyen, Élevé, Très élevé (xhigh) ou Max
- Effort de raisonnement spécifique au fournisseur
- Modes de raisonnement fixes pour les modèles qui ne permettent pas la sélection par l'utilisateur

Lors du changement de modèle, Anthro Bridge tente de préserver le paramètre de raisonnement compatible le plus proche. Si le paramètre précédent exact n'est pas disponible, il sélectionne l'option prise en charge la plus proche, en préférant l'option la plus faible lorsque deux choix sont également proches.

### Détection des capacités

Anthro Bridge combine un registre de capacités intégré avec les métadonnées en direct d'OpenRouter.

Les capacités peuvent inclure :

- Entrée d'image
- Entrée vidéo
- Support du mode Thinking
- Support de l'effort de raisonnement
- Tarification connue
- Règles de traduction de requête spécifiques au fournisseur

Les métadonnées en direct d'OpenRouter sont mises en cache pour réduire les appels API inutiles.

### Normalisation du modèle de réponse

Les API amont renvoient souvent leur propre nom de modèle dans les réponses. Anthro Bridge peut réécrire ce champ pour revenir au nom de route Anthropic attendu par le client.

Par exemple :

```text
Modèle de réponse amont : deepseek-v4-pro
Modèle visible par le client : claude-sonnet-5
```

La normalisation s'applique aux réponses en streaming et non-streaming et peut être activée ou désactivée dans les Paramètres.

### Écritures de configuration sérialisées

Les mutations de configuration sont sérialisées pour empêcher les écritures concurrentes de corrompre ou d'annuler les paramètres.

Cela couvre les opérations telles que :

- Les changements de modèle
- Les changements de mode Thinking
- Les changements d'effort de raisonnement
- Les changements de profil OpenRouter
- Les changements de configuration liés aux clés API

### File d'attente de sauvegarde OpenRouter

Les changements de route OpenRouter sont traités via une file d'attente de sauvegarde dédiée.

La file d'attente fournit :

- Des opérations de sauvegarde sérialisées
- La suppression des requêtes obsolètes
- L'identité de la route capturée au moment de la soumission d'une requête
- Une protection contre les fermetures (closures) React obsolètes
- Une protection contre le retour en arrière (rollback) depuis une route précédemment sélectionnée
- Une nouvelle tentative d'actualisation après une sauvegarde réussie
- Une gestion agrégée du redémarrage de la passerelle
- Un traitement sûr des requêtes ajoutées pendant le travail post-sauvegarde

Cela empêche les changements rapides de modèle, les changements de route ou les réponses Tauri différées de restaurer d'anciennes valeurs de l'interface.

### Gestion de la passerelle

L'interface graphique fournit :

- Des contrôles de démarrage et d'arrêt de la passerelle
- La sélection du fournisseur et du profil
- La configuration des routes
- La gestion des clés API
- La visualisation des journaux
- L'actualisation de la liste des modèles
- L'état de la sauvegarde et l'affichage des erreurs

La passerelle écoute sur :

```text
http://127.0.0.1:4000
```

## Prérequis

- Windows 10 ou Windows 11
- Node.js 24 ou ultérieur pour le développement
- Une chaîne d'outils Rust stable pour le développement
- Une clé API pour au moins un fournisseur pris en charge

Une seule clé fournisseur suffit. Vous n'avez pas besoin de clés pour chaque fournisseur.

## Installation

Téléchargez le dernier installateur Windows depuis la page Releases du projet et exécutez-le.

L'installateur prend en charge :

- Anglais
- Japonais
- Chinois simplifié
- Chinois traditionnel
- Coréen
- Français
- Allemand
- Espagnol

Pour mettre à jour Anthro Bridge, exécutez l'installateur le plus récent. Les paramètres utilisateur existants sont conservés.

La configuration utilisateur stable est stockée sous :

```text
%APPDATA%\Anthro Bridge\
```

Les versions de développement utilisent une identité d'application et un répertoire de données distincts :

```text
%APPDATA%\Anthro Bridge Dev\
```

Cela permet aux versions stable et de développement de coexister sans partager les fichiers de configuration ou de cache.

## Démarrage rapide

### 1. Configurer une clé API

Ouvrez :

```text
Paramètres > Clé API
```

Saisissez la clé du fournisseur que vous prévoyez d'utiliser et enregistrez-la.

Les noms de variables d'environnement courants sont :

| Fournisseur | Variable d'environnement |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

Les profils OpenRouter peuvent utiliser des paramètres de clé spécifiques au profil, gérés via l'interface graphique.

### 2. Configurer les modèles de route

Ouvrez les Paramètres et sélectionnez le modèle amont pour chaque route :

- Opus
- Sonnet
- Haiku

Pour OpenRouter, sélectionnez ou créez d'abord un profil, puis configurez chaque route à l'intérieur de ce profil.

### 3. Démarrer la passerelle

Cliquez sur **Démarrer la passerelle**.

Vérifiez que le point de terminaison local est disponible :

```text
GET http://127.0.0.1:4000/health
```

### 4. Configurer Claude Desktop ou Claude Code

Dirigez le client vers le point de terminaison Anthro Bridge tout en continuant à utiliser les noms de modèles Anthropic.

Des instructions détaillées pour l'inférence tierce sont disponibles dans :

```text
docs/THIRD_PARTY_INFERENCE.md
```

## Points de terminaison API

| Méthode | Chemin | Description |
|---|---|---|
| `GET` | `/health` | Vérification de l'état de la passerelle |
| `GET` | `/v1/models` | Liste publique des modèles de route |
| `POST` | `/v1/messages` | API Messages en streaming et non-streaming |
| `POST` | `/v1/messages/count_tokens` | Comptage de jetons lorsque pris en charge par le fournisseur sélectionné |

## Configuration

Le fichier de configuration principal est `config.json`.

La plupart des paramètres doivent être modifiés via l'interface graphique. L'édition manuelle est destinée à un usage avancé.

Les champs de modèle importants incluent :

| Clé | Description |
|---|---|
| `models.<route>.upstream_model` | Nom du modèle amont envoyé au fournisseur |
| `models.<route>.thinking_mode` | Mode Thinking spécifique à la route |
| `models.<route>.reasoning_effort` | Effort de raisonnement spécifique au fournisseur |
| `models.<route>.supports_vision` | Remplacement du support d'image |
| `models.<route>.supports_video` | Remplacement du support vidéo |
| `models.<route>.visible` | Si la route est exposée aux clients et au tableau de bord |
| `non_vision_image_policy` | Comment les entrées d'image non prises en charge sont traitées |
| `normalize_response_model_identity` | Si les noms de modèles de réponse sont normalisés |

Les images non prises en charge peuvent être traitées selon l'une des politiques suivantes :

- `replace` : remplacer l'image par un texte de substitution
- `drop` : supprimer le contenu de l'image
- `reject` : renvoyer une erreur

## Notes sur les fournisseurs

### DeepSeek

Les modèles DeepSeek Pro peuvent utiliser un effort de raisonnement configurable. Les modèles Flash n'exposent pas le même contrôle d'effort de raisonnement, donc les options indisponibles sont désactivées automatiquement.

### MiniMax

Le comportement des modèles MiniMax diffère selon la génération du modèle. Anthro Bridge applique le format de requête requis par le modèle sélectionné, y compris le mode Thinking adaptatif ou désactivé lorsque pris en charge.

### Kimi

Les modèles Kimi peuvent utiliser soit un paramètre de réflexion (thinking), soit un mode d'effort de raisonnement fixe selon la famille du modèle. Anthro Bridge traduit la sélection de l'interface graphique dans le format de requête amont approprié.

### MiMo

MiMo utilise `thinking_mode` plutôt que le champ générique `thinking` pour les routes prises en charge.

Le support de la vision varie selon le modèle. Anthro Bridge applique la politique d'image non prise en charge configurée lorsqu'une route ne peut pas accepter d'entrée d'image.

### OpenRouter

Les modèles OpenRouter sont regroupés par fournisseur lorsqu'ils sont reconnus. L'interface graphique fournit :

- La recherche de modèle
- Le regroupement par fournisseur
- La saisie personnalisée de modèle
- Des badges de capacité
- L'affichage des prix
- Des contrôles de raisonnement par modèle
- Une actualisation unifiée de la liste des modèles

Les capacités et le comportement des modèles OpenRouter peuvent évoluer dans le temps. Les métadonnées en direct sont utilisées lorsqu'elles sont disponibles, tandis que le registre intégré fournit des valeurs par défaut stables pour les modèles connus.

## Interface utilisateur

L'interface des Paramètres inclut :

- Des sections de fournisseur rétractables
- La configuration des routes Opus, Sonnet et Haiku
- La recherche de modèle et le regroupement par fournisseur pour OpenRouter
- Des contrôles de réflexion (thinking) et de raisonnement basés sur la capacité du modèle
- La saisie personnalisée de modèle amont
- La sauvegarde automatique des routes
- La sauvegarde explicite des clés API
- La progression de la sauvegarde et les messages d'erreur
- Les informations de prix et de capacité des modèles
- Le bouton de normalisation du modèle de réponse

Le Tableau de bord inclut :

- La sélection du fournisseur ou du profil OpenRouter
- L'état de la passerelle
- Les mappages de route actuels
- Des indicateurs de capacité
- Les informations de prix
- L'état de commutation du fournisseur

## Développement

### Structure du projet

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
│   │   │   └── paths.rs
│   │   └── resources/
│   └── package.json
└── LICENSE
```

### Exécuter en mode développement

```bash
cd gui
npm install
npm run tauri dev
```

### Compiler la variante de développement

Sur Windows, utilisez une seule tâche de compilation Rust pour éviter l'arrêt intermittent du compilateur :

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Les versions de développement utilisent :

- Titre de fenêtre : `Anthro Bridge (DEV)`
- Port : `4000`
- Identité d'application : `com.soheidon.anthro-bridge.dev`
- Répertoires de configuration et de cache distincts

### Versions stables

Les versions stables ne doivent être créées que pour la préparation d'une release. Le travail normal d'implémentation et de vérification doit utiliser la variante de développement.

## Vérification

Vérification frontend :

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Vérification Rust :

```bash
cd gui/src-tauri
cargo check
```

Pour le sélecteur de route OpenRouter spécifiquement :

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

Les tests du sélecteur OpenRouter couvrent :

- L'identité de la route capturée lors des sauvegardes en file d'attente
- La protection contre le retour en arrière entre routes
- La protection contre les callbacks obsolètes
- Le comportement de nouvelle tentative d'actualisation
- Le redémarrage de la passerelle après échec d'actualisation
- La suppression des requêtes en vol
- La suppression du retour en arrière basé sur la génération

Un test multi-sauvegarde dédié pour l'agrégation de redémarrage pourrait être ajouté pour verrouiller le comportement suivant :

```text
sauvegarde 1 demande un redémarrage
sauvegarde 2 ne demande pas de redémarrage
résultat : redémarrage unique après le lot
```

## Liste de vérification manuelle

Les tests automatisés ne reproduisent pas toutes les conditions de temporisation de Tauri et React. Avant la release, vérifiez les éléments suivants dans la version de développement :

- Chaque profil OpenRouter affiche les détails corrects au survol
- La sélection de modèle ne revient pas visiblement en arrière après un changement
- Les sélections de Thinking et de raisonnement restent stables après la sauvegarde
- Les paramètres restent corrects après la fermeture et la réouverture de l'écran des paramètres
- Les paramètres restent corrects après le redémarrage de l'application
- Le changement de profil pendant une sauvegarde ne corrompt aucun des deux profils
- Une sauvegarde échouée ne fait revenir en arrière que la route qui l'a initiée
- Une nouvelle tentative d'actualisation réussie efface l'erreur précédente
- Une nouvelle tentative d'actualisation échouée laisse la dernière erreur visible
- Le redémarrage requis de la passerelle se produit une seule fois après le lot
- Les modèles personnalisés se sauvegardent et se rechargent correctement
- Les capacités intégrées et en direct d'OpenRouter sont affichées correctement

## Dépannage

### Le port 4000 est déjà utilisé

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Un modèle rejette l'entrée d'image ou de vidéo

Les capacités des modèles varient selon le fournisseur et la route. Vérifiez les badges de capacité dans l'interface graphique et sélectionnez une route compatible.

Pour les entrées d'image non prises en charge, Anthro Bridge suit la `non_vision_image_policy`.

### Les paramètres sont réinitialisés après une mise à niveau

Redémarrez d'abord l'application pour que les migrations puissent s'exécuter.

Si le problème persiste :

1. Sauvegardez la configuration utilisateur.
2. Comparez-la avec la configuration fournie.
3. Supprimez les champs obsolètes ou réinitialisez la configuration utilisateur si nécessaire.

Emplacement de la configuration stable :

```text
%APPDATA%\Anthro Bridge\config.json
```

Emplacement de la configuration de développement :

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### La liste des modèles OpenRouter est obsolète

Utilisez le contrôle d'actualisation unifié des modèles dans les Paramètres. Anthro Bridge met en cache les métadonnées des modèles, donc une actualisation manuelle peut être nécessaire après qu'OpenRouter a modifié une entrée de modèle.

## Traduction

L'anglais est le README source.

Les fichiers README traduits sont stockés sous `docs/`. Lorsque le README anglais change, régénérez ou mettez à jour les fichiers traduits à partir de la source anglaise plutôt que de modifier chaque langue indépendamment.

Les fichiers de langue pour l'interface de l'application sont stockés sous :

```text
gui/src/i18n/lang/
```

## Licence

Licence MIT. Voir [LICENSE](../LICENSE).
