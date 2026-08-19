# 1-1 OpenRouter Gemini Support Plan

## 目的
Anthro Bridge に OpenRouter 経由の Gemini 対応を追加し、Settings の OpenRouter profile 一覧へ built-in profile `OpenRouter: Gemini` を自動追加する。

## 初期対応モデル
- google/gemini-3.1-pro-preview
- google/gemini-3.7-flash

## 初期ルーティング（全スロット Gemini 3.7 Flash に統一）
- Opus 5 → google/gemini-3.7-flash + High
- Sonnet 5 → google/gemini-3.7-flash + High
- Haiku 4.5 → google/gemini-3.7-flash + Low
- Gemini 3.1 Pro Preview は選択可能モデルとして残すが、built-in デフォルトには使わない

## Reasoning 方針
- 3.1 Pro Preview / 3.7 Flash: Low / Medium / High
- Normal / Off / Minimal / XHigh / Max は表示しない
- xhigh / max / minimal は互換性のため high へ正規化
- backend は既存 3.5 Flash Lite プロファイルの後方互換のため minimal 認識を維持（UI の built-in list からは削除）

## 実装済み変更
- builtinOpenRouter.ts: Google vendor + Gemini 2 モデルを追加（3.5 Flash Lite は built-in list から削除）
- modelCapabilities.ts: minimal 型と provider model list を追加（3.5 Flash Lite は削除）
- OpenRouterModelSelector.tsx: Google vendor グループ（visibleModelOptions に google 分岐を group/other 両モードで追加。GEMINI_MODEL_IDS による allow-list を最終適用）と Gemini Thinking options を追加
- model_capabilities.rs: static capability と is_gemini_model を追加（3.5 Flash Lite は後方互換のため認識を維持）
- proxy.rs: Gemini reasoning.effort 変換を追加（OpenRouter共通 reasoning envelope の兄弟 if ブロック。Google 固有 payload への変換は行わず effort 値の正規化のみ）
- lib.rs: OpenRouter: Gemini プロファイル builder / ensure を追加（全スロット 3.7 Flash、BUILTIN_NAMES に "OpenRouter: Gemini" を追加）
- model_context_windows.json: Gemini 2 モデルの 1M context を追加（3.5 Flash Lite は削除）
- resources/config.json: 新規インストール用テンプレートへ Gemini profile 追加（全スロット 3.7 Flash）
- i18n: groupGoogle を 8 言語へ追加

## 調査結果（レビュー指摘 3 点）
### 1. proxy.rs の Gemini reasoning.effort 変換 — 最小限と確認
- `proxy.rs:2361-2377` に OpenRouter 共通 `reasoning` envelope（`{"reasoning":{"effort":...}}`）の兄弟 if ブロックとして実装。
- `normalize_gemini_reasoning_effort`（`proxy.rs:2853-2864`）が effort 値を正規化（xhigh/max→high、minimal は 3.5 Flash Lite のみ維持、その他→high）。
- Google 固有 payload（thinkingConfig / thinkingBudget 等）への変換は行っていない。OpenRouter の共通 reasoning API に乗る。
- 判定: 問題なし。過剰実装ではない。

### 2. reasoning_details の扱い — proxy response経路では透過、multi-turn end-to-end は未確認
- Anthro Bridge コード内に `reasoning_details` を明示的に削除・変換する処理はない。
- streaming response では model 名以外の SSE frame を透過し、non-streaming でも model 以外のフィールドを保持するため、OpenRouter から返された `reasoning_details` を Anthro Bridge 自身が response 時に落とす可能性は低い。
- ただし multi-turn tool calling で必要なのは、Claude Code 側がその情報を保持して次ターン request へ戻し、Anthro Bridge 経由で再び OpenRouter へ渡せることまで含む。
- Anthro Bridge はステートレスであり、自ら `reasoning_details` を保存・再注入しない。
- したがって、コード調査だけでは end-to-end preservation を保証できない。
- 判定: proxy response 透過については問題なし。multi-turn 互換性は実 API smoke で確認するまで未確定。現時点では追加実装を行わない。

### 3. Vitest 1 failure — 既存の failure（Gemini 起因ではない）
- 失敗: `modelContextWindows.test.ts` の `every direct provider model has context metadata`。
- 原因: `modelCapabilities.ts` の deepseek に date-variant（`deepseek-v4-pro (from 2026-08-16)` / `deepseek-v4-flash (from 2026-08-16)`）が追加されたが、`model_context_windows.json` に未登録。
- Gemini 3 モデルは context metadata 完全一致（登録済み）。Gemini 起因の失敗はなし。
- 対応済み: `model_context_windows.json` に上記 2 エントリを追加（Gemini とは独立した別タスクとして実施）。

## 残タスク
- [x] `model_context_windows.json` に DeepSeek date-variant 2 エントリ追加（vitest failure 解消。Gemini とは独立）
- [ ] 実機 smoke: Gemini 3.1 Pro Preview の multi-turn tool calling を Claude Code から実行し、1回目の tool call 後の2回目リクエストまで正常継続することを確認する（モデルが tool を要求 → Claude Code が tool 実行 → 次ターン送信 → Gemini が正常に続きを生成、まで成功するのが本当の smoke）
- [ ] reasoning_details への追加対応は smoke 結果次第で判断（現時点では未実装）

## Verification
- cargo check: pass
- cargo test: 403 passed
- tsc --noEmit: pass
- vitest: 98 passed（DeepSeek date-variant context metadata 追加済み。Gemini 起因の失敗なし）

## 注意
- コミットはしていない
- 実機 smoke は未実施
- reasoning_details への追加実装は未実施（smoke 結果待ち）
