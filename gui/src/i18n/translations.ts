export const translations = {
  en: {
    // Header
    "header.title": "Anthropic Provider Gateway Manager",
    "header.gatewayRunning": "Gateway: Running",
    "header.gatewayStopped": "Gateway: Stopped",
    "header.gatewayDetected": "Gateway: Detected",
    "header.gatewayUnreachable": "Gateway: Disconnected",
    "header.startGateway": "Start Gateway",
    "header.stopGateway": "Stop Gateway",
    "status.unknown": "Unknown",

    // Tab bar
    "tab.dashboard": "Dashboard",
    "tab.gatewaySettings": "Gateway Settings",
    "tab.claudeSetup": "Claude Desktop Setup",
    "tab.apiKey": "API Key",

    // StatusPanel
    "statusPanel.header": "Status",
    "statusPanel.ok": "OK",
    "statusPanel.unreachable": "UNREACHABLE",
    "statusPanel.upstream": "Upstream:",
    "statusPanel.port4000": "Port 4000",
    "statusPanel.listening": "LISTENING",
    "statusPanel.notListening": "Not listening",
    "statusPanel.apiKey": "API Key",
    "statusPanel.set": "Set",
    "statusPanel.notSet": "Not set",
    "statusPanel.gatewayUrl": "Gateway URL",
    "statusPanel.gatewayUrlValue": "http://127.0.0.1:4000",

    // ConfigPanel
    "configPanel.header": "Config (config.json)",
    "configPanel.reload": "Reload",
    "configPanel.noConfig": "No config loaded",
    "configPanel.save": "Save",
    "configPanel.saved": "Saved!",
    "configPanel.saveError": "Save failed",
    "configPanel.currentEncoding": "Current: {enc}",
    "configPanel.willChangeEncoding": "(will save as {enc})",
    "configPanel.recommended": "Japanese Windows requires Shift-JIS",

    // ClaudeConfigPanel
    "claudeConfig.header": "Claude Desktop Gateway Config",
    "claudeConfig.copy": "Copy to Clipboard",
    "claudeConfig.copied": "Copied!",
    "claudeConfig.helpText": "Paste this configuration into your Claude Desktop settings file:",
    "claudeConfig.openFolder": "Open Folder",
    "claudeConfig.openFile": "Open File",
    "claudeConfig.browseManually": "Browse manually",
    "claudeConfig.discoveryTitle": "Detected Config Files",
    "claudeConfig.noFilesFound": "No Claude config files found. Use the folders above to browse manually.",
    "claudeConfig.searching": "Searching...",
    "claudeConfig.likelyConfig": "Likely Claude config",

    // LogPanel
    "logPanel.header": "Latest Log",
    "logPanel.reload": "Reload",
    "logPanel.proCount": "Pro: {count}",
    "logPanel.flashCount": "Flash: {count}",
    "logPanel.lines": "Lines: {count}",
    "logPanel.emptyLog": "(empty log)",
    "logPanel.openFolder": "Open Logs Folder",
    "logPanel.noLog": "No log loaded",
    "logPanel.selectLog": "Switch to another log...",
    "logPanel.newLog": "New Log",

    // ApiKeyPanel
    "apiKeyPanel.header": "API Key",
    "apiKeyPanel.placeholder": "sk-...",
    "apiKeyPanel.save": "Save",
    "apiKeyPanel.saved": "Key saved",
    "apiKeyPanel.set": "Set",
    "apiKeyPanel.notSet": "Not set",
    "apiKeyPanel.helpText": "Sets the provider API key as a user environment variable (persistent across restarts).",
    "apiKeyPanel.activeProvider": "Active provider",
    "apiKeyPanel.envVarLabel": "Env var name",
    "apiKeyPanel.envVarSave": "Save Name",
    "apiKeyPanel.envVarSaved": "Name saved",
    "apiKeyPanel.envVarHelpText": "Name of the environment variable to store the API key (uppercase, digits, underscores).",
  },

  ja: {
    // Header
    "header.title": "Anthropic Provider Gateway Manager",
    "header.gatewayRunning": "ゲートウェイ: 実行中",
    "header.gatewayStopped": "ゲートウェイ: 停止中",
    "header.gatewayDetected": "ゲートウェイ: 外部で動作中",
    "header.gatewayUnreachable": "ゲートウェイ: 未接続",
    "header.startGateway": "ゲートウェイ起動",
    "header.stopGateway": "ゲートウェイ停止",
    "status.unknown": "不明",

    // Tab bar
    "tab.dashboard": "ダッシュボード",
    "tab.gatewaySettings": "ゲートウェイ設定",
    "tab.claudeSetup": "Claude Desktop 設定",
    "tab.apiKey": "APIキー",

    // StatusPanel
    "statusPanel.header": "ステータス",
    "statusPanel.ok": "正常",
    "statusPanel.unreachable": "到達不可",
    "statusPanel.upstream": "Upstream:",
    "statusPanel.port4000": "ポート 4000",
    "statusPanel.listening": "待受中",
    "statusPanel.notListening": "待受なし",
    "statusPanel.apiKey": "APIキー",
    "statusPanel.set": "設定済",
    "statusPanel.notSet": "未設定",
    "statusPanel.gatewayUrl": "Gateway URL",
    "statusPanel.gatewayUrlValue": "http://127.0.0.1:4000",

    // ConfigPanel
    "configPanel.header": "設定 (config.json)",
    "configPanel.reload": "再読込",
    "configPanel.noConfig": "設定が読み込まれていません",
    "configPanel.save": "保存",
    "configPanel.saved": "保存しました",
    "configPanel.saveError": "保存に失敗しました",
    "configPanel.currentEncoding": "現在: {enc}",
    "configPanel.willChangeEncoding": "(保存時に{enc}に変更)",
    "configPanel.recommended": "日本語WindowsでのエンコードはShift-JIS",

    // ClaudeConfigPanel
    "claudeConfig.header": "Claude Desktop ゲートウェイ設定",
    "claudeConfig.copy": "クリップボードにコピー",
    "claudeConfig.copied": "コピーしました",
    "claudeConfig.helpText": "この設定を Claude Desktop の設定ファイルに貼り付けてください：",
    "claudeConfig.openFolder": "フォルダを開く",
    "claudeConfig.openFile": "ファイルを開く",
    "claudeConfig.browseManually": "手動で探す",
    "claudeConfig.discoveryTitle": "検出された設定ファイル",
    "claudeConfig.noFilesFound": "Claude 設定ファイルが見つかりません。上のフォルダから手動で探してください。",
    "claudeConfig.searching": "検索中...",
    "claudeConfig.likelyConfig": "Claude設定の可能性高",

    // LogPanel
    "logPanel.header": "最新ログ",
    "logPanel.reload": "再読込",
    "logPanel.proCount": "Pro: {count}",
    "logPanel.flashCount": "Flash: {count}",
    "logPanel.lines": "行数: {count}",
    "logPanel.emptyLog": "(空のログ)",
    "logPanel.openFolder": "ログフォルダを開く",
    "logPanel.noLog": "ログが読み込まれていません",
    "logPanel.selectLog": "別のログに切り替え...",
    "logPanel.newLog": "新規ログ",

    // ApiKeyPanel
    "apiKeyPanel.header": "APIキー",
    "apiKeyPanel.placeholder": "sk-...",
    "apiKeyPanel.save": "保存",
    "apiKeyPanel.saved": "キーを保存しました",
    "apiKeyPanel.set": "設定済",
    "apiKeyPanel.notSet": "未設定",
    "apiKeyPanel.helpText": "プロバイダのAPIキーをユーザー環境変数として設定します（再起動後も保持されます）。",
    "apiKeyPanel.activeProvider": "アクティブプロバイダ",
    "apiKeyPanel.envVarLabel": "環境変数名",
    "apiKeyPanel.envVarSave": "名前を保存",
    "apiKeyPanel.envVarSaved": "名前を保存しました",
    "apiKeyPanel.envVarHelpText": "APIキーを保存する環境変数の名前（大文字、数字、アンダースコア）。",
  },
} as const;

export type TranslationKey = keyof typeof translations.en;
export type Lang = "ja" | "en";
