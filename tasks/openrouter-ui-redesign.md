# Plan: OpenRouter Model Selector UI Redesign — Vendor Hierarchy

## Context

現在の `OpenRouterModelSelector` は 400+ モデルを固定 8 グループ（recommended, anthropic, openai, google, deepseek, moonshot, qwen, other）のフラットリストで表示している。ユーザーの要望: **vendor（会社・提供元）を上位階層、モデルを下位階層とする 2 階層構造**に変更し、折りたたみ可能なアコーディオンで表示する。

## Design

### 表示構造

```
🔍 モデル名またはIDを検索          [↻]

Anthropic (3)                          ← 折りたたみヘッダー（件数付き）
  ─ Claude Opus Latest          $3/1M  ← モデル行
  ─ Claude Sonnet Latest        $3/1M
  ─ Claude Haiku Latest        $0.25/1M

OpenAI (24)
  ─ GPT-5.2                   $2.5/1M
  ─ GPT-5.2 Codex              $1/1M

Poolside (1)
  ─ Laguna S 2.1              $0.2/1M

+ カスタムモデル（一覧にない場合）
```

### 選択時のトリガー表示

```
[ Anthropic › Claude Sonnet Latest  ▾ ]
```

### 分類ロジック変更

**現在**: 8つのハードコードグループ + `classifyModel()` の prefix マッチ
**新規**: `model.id` の `/` より前を vendor ID として自動分類

```ts
function getVendorId(modelId: string): string {
  const normalized = modelId.startsWith("~") ? modelId.slice(1) : modelId;
  return normalized.split("/")[0] || "other";
}
```

vendor ID → 表示名マッピング（`VENDOR_LABELS`）+ 未知 vendor は自動フォーマット

特殊ケース: `openrouter/auto`, `openrouter/free` → 「OpenRouter ルーター」グループ

### アコーディオン挙動

- 初期状態: 選択中モデルの vendor + 推奨モデルの vendor を開く
- 検索中: ヒットモデルを含む vendor を自動的に開く
- クリックで開閉トグル
- vendor ヘッダーにモデル件数を表示: `Anthropic (3)`

### 検索対象

`model.displayName` + `model.id` + `vendorLabel` + `model.description` のすべて

## Files Modified

| File | Change |
|------|--------|
| `gui/src/components/OpenRouterModelSelector.tsx` | UI 全面リライト（分類ロジック + アコーディオン + トリガー表示） |

### 変更しないもの

- モデル保存処理 (`invoke("set_model_upstream")`)
- キャッシュ取得 (`invoke("openrouter_get_models")`)
- 料金表示
- Custom model 入力
- Stale/Warning 表示
- i18n キー（追加のみ、既存は変更なし）
- バックエンド Rust コード

## Implementation

### Step 1: 分類ロジックの置き換え

`classifyModel()` + `GROUP_ORDER` + `GROUP_LABELS` を削除し、以下に置き換え:

```ts
const VENDOR_LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  google: "Google",
  deepseek: "DeepSeek",
  moonshotai: "Moonshot AI",
  qwen: "Qwen",
  cohere: "Cohere",
  poolside: "Poolside",
  minimax: "MiniMax",
  mistralai: "Mistral AI",
  x_ai: "xAI",
  openrouter: "OpenRouter",
};

function getVendorId(modelId: string): string {
  const n = modelId.startsWith("~") ? modelId.slice(1) : modelId;
  return n.split("/")[0] || "other";
}

function formatVendorName(id: string): string {
  return VENDOR_LABELS[id]
    ?? id.split(/[-_]/).map(p => p.charAt(0).toUpperCase() + p.slice(1)).join(" ");
}

function isRouterModel(modelId: string): boolean {
  const n = modelId.startsWith("~") ? modelId.slice(1) : modelId;
  const [vendor, ...rest] = n.split("/");
  return vendor === "openrouter" && rest.length <= 1;
}
```

### Step 2: `grouped` useMemo を書き換え

```ts
const grouped = useMemo(() => {
  const vendors = new Map<string, { label: string; items: OpenRouterModel[] }>();
  const routerItems: OpenRouterModel[] = [];

  for (const m of models) {
    if (isRouterModel(m.id)) {
      routerItems.push(m);
      continue;
    }
    const vid = getVendorId(m.id);
    if (!vendors.has(vid)) vendors.set(vid, { label: formatVendorName(vid), items: [] });
    vendors.get(vid)!.items.push(m);
  }

  // Sort vendors by count descending, then alphabetically
  const sorted = [...vendors.entries()]
    .sort((a, b) => b[1].items.length - a[1].items.length || a[0].localeCompare(b[0]));

  // Sort within each vendor
  for (const [_, v] of sorted) v.items.sort((a, b) => a.displayName.localeCompare(b.displayName));

  const result: { key: string; label: string; count: number; items: OpenRouterModel[] }[] = [];

  // Recommended group first
  const recItems = models.filter(m => RECOMMENDED_MODELS.has(m.id));
  if (recItems.length > 0) {
    result.push({ key: "__recommended", label: t("openRouterModels.groupRecommended"), count: recItems.length, items: recItems });
  }

  for (const [vid, v] of sorted) {
    result.push({ key: vid, label: v.label, count: v.items.length, items: v.items });
  }

  if (routerItems.length > 0) {
    routerItems.sort((a, b) => a.displayName.localeCompare(b.displayName));
    result.push({ key: "__router", label: t("openRouterModels.groupRouter"), count: routerItems.length, items: routerItems });
  }

  return result;
}, [models, t]);
```

### Step 3: アコーディオン状態管理

```ts
const [expandedVendors, setExpandedVendors] = useState<Set<string>>(new Set());

// 初期展開: 選択中モデルの vendor + recommended
useEffect(() => {
  const initial = new Set<string>();
  initial.add("__recommended");
  if (selectedModelId) {
    initial.add(getVendorId(selectedModelId));
  }
  setExpandedVendors(initial);
}, []); // 初回のみ

const toggleVendor = useCallback((key: string) => {
  setExpandedVendors(prev => {
    const next = new Set(prev);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    return next;
  });
}, []);
```

### Step 4: 検索時は該当 vendor を自動展開

```ts
const filteredGroups = useMemo(() => {
  const q = search.trim().toLowerCase();
  if (!q) return grouped;

  const filtered = grouped
    .map(g => ({
      ...g,
      items: g.items.filter(m => {
        const haystack = [m.displayName, m.id, g.label, m.description].filter(Boolean).join(" ").toLowerCase();
        return haystack.includes(q);
      }),
    }))
    .filter(g => g.items.length > 0);

  // Auto-expand filtered vendors
  setExpandedVendors(new Set(filtered.map(g => g.key)));
  return filtered;
}, [grouped, search]);
```

### Step 5: トリガー表示を変更

```ts
const selectedVendorLabel = useMemo(() => {
  if (showCustom && customText) return customText;
  const found = models.find(m => m.id === selectedModelId);
  if (found) return `${formatVendorId(getVendorId(found.id))} › ${found.displayName}`;
  return selectedModelId;
}, [models, selectedModelId, showCustom, customText]);
```

### Step 6: JSX レンダリング

アコーディオンヘッダー + モデル行の JSX を書き換え:

```tsx
{/* Vendor groups */}
{filteredGroups.map((group) => (
  <div key={group.key} className="openrouter-vendor">
    <div className="openrouter-vendor-header"
         onClick={() => toggleVendor(group.key)}>
      <span>{expandedVendors.has(group.key) ? "▾" : "▸"}</span>
      <span>{group.label}</span>
      <span>({group.count})</span>
    </div>
    {expandedVendors.has(group.key) && group.items.map((model) => (
      <div key={model.id} className="openrouter-model-item" ...>
        <span>{model.displayName}</span>
        <span>{model.pricing.prompt}/1M</span>
      </div>
    ))}
  </div>
))}
```

### Step 7: i18n キー追加

```ts
"openRouterModels.groupRouter": "OpenRouter ルーター",  // 各言語に追加
```

8 言語ファイルに追加。

## Verification

1. `npx tsc --noEmit` — エラーなし
2. `cargo check` — 変更なし（Rust 未変更）
3. プレビューで動作確認:
   - モデル一覧が vendor 階層で表示される
   - アコーディオンの開閉が機能する
   - 検索でヒットした vendor が自動展開される
   - 選択中モデルの vendor は初期展開される
   - トリガー表示が `Anthropic › Claude Sonnet Latest` 形式
   - 件数表示が正しい
   - Custom model 入力はそのまま機能する
   - 互換性警告はそのまま機能する
