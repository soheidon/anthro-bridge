import { useRef, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLog } from "../hooks/useLog";
import { useTranslation } from "../i18n";
import type { LogContent, LogListEntry } from "../types";

export default function LogPanel() {
  const { t } = useTranslation();
  const { data, error, loading, refresh } = useLog();
  const scrollRef = useRef<HTMLPreElement>(null);

  // Log file switching
  const [logs, setLogs] = useState<LogListEntry[]>([]);
  const [selectedLog, setSelectedLog] = useState<string | null>(null);
  const [logContent, setLogContent] = useState<LogContent | null>(null);
  const [logLoading, setLogLoading] = useState(false);

  // Collapse toggle
  const [collapsed, setCollapsed] = useState(true);

  // Load log list on mount
  useEffect(() => {
    loadLogList();
  }, []);

  const loadLogList = () => {
    invoke<LogListEntry[]>("list_logs")
      .then(setLogs)
      .catch(console.error);
  };

  // Sync useLog data when no specific file is selected
  useEffect(() => {
    if (!selectedLog && data) {
      setLogContent(data);
    }
  }, [data, selectedLog]);

  // Determine which content to display
  const display = selectedLog ? logContent : data;

  useEffect(() => {
    if (display && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [display]);

  // Count Pro vs Flash requests in the displayed log
  const { proCount, flashCount } = useMemo(() => {
    if (!display?.content) return { proCount: 0, flashCount: 0 };
    const pro = (display.content.match(/-> deepseek-v4-pro/g) || []).length;
    const flash = (display.content.match(/-> deepseek-v4-flash/g) || []).length;
    return { proCount: pro, flashCount: flash };
  }, [display]);

  const openFolder = () => {
    invoke("open_logs_folder").catch(console.error);
  };

  const handleLogSwitch = (filename: string) => {
    if (!filename) {
      setSelectedLog(null);
      setLogContent(null);
      return;
    }
    setSelectedLog(filename);
    setLogLoading(true);
    invoke<LogContent>("read_log", { filename })
      .then((result) => { setLogContent(result); setLogLoading(false); })
      .catch((e) => { console.error(e); setLogLoading(false); });
  };

  const handleNewLog = async () => {
    try {
      const newFilename = await invoke<string>("create_new_log");
      await loadLogList();
      setSelectedLog(newFilename);
      setLogLoading(true);
      const content = await invoke<LogContent>("read_log", { filename: newFilename });
      setLogContent(content);
      setLogLoading(false);
    } catch (e) {
      console.error(e);
    }
  };

  // Remove Pro/Flash labels from log lines for cleaner display
  const logLines = useMemo(() => {
    if (!display?.content) return "";
    return display.content
      .replace(/\[Sonnet 4\.6\]\s*/g, "")
      .replace(/\[Haiku 4\.5\]\s*/g, "");
  }, [display]);

  return (
    <div className="panel log-panel">
      <div className="panel-header">
        <button
          className="collapse-header"
          onClick={() => setCollapsed(!collapsed)}
          style={{ border: "none", background: "none", cursor: "pointer", padding: 0, fontSize: "inherit", fontWeight: 600, color: "inherit" }}
        >
          <span>{collapsed ? "▶" : "▼"}</span>
          {t("logPanel.header")}
          {display?.filename && !selectedLog && (
            <span style={{ color: "var(--text-muted)", marginLeft: 8, fontWeight: 400 }}>
              {display.filename}
            </span>
          )}
        </button>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <select
            className="log-file-selector"
            value={selectedLog ?? ""}
            onChange={(e) => handleLogSwitch(e.target.value)}
          >
            <option value="">{t("logPanel.selectLog")}</option>
            {logs.map((l) => (
              <option key={l.filename} value={l.filename}>
                {l.filename}
              </option>
            ))}
          </select>
          <button className="btn btn-small" onClick={handleNewLog}>
            {t("logPanel.newLog")}
          </button>
          <button className="btn btn-small" onClick={openFolder}>
            {t("logPanel.openFolder")}
          </button>
          <button className="btn btn-small" onClick={refresh}>
            {t("logPanel.reload")}
          </button>
        </div>
      </div>

      {/* Log body: collapsible */}
      {!collapsed && (
        <div className="panel-content">
          {(loading || logLoading) ? (
            <div className="loading" />
          ) : error ? (
            <div className="error-text">{error}</div>
          ) : display ? (
            <>
              <div className="pro-flash-summary">
                <span className="pro">{t("logPanel.proCount", { count: proCount })}</span>
                <span className="flash">{t("logPanel.flashCount", { count: flashCount })}</span>
                <span style={{ color: "var(--text-muted)" }}>
                  {t("logPanel.lines", { count: display.line_count })}
                </span>
              </div>
              <pre ref={scrollRef} className="log-content compact">
                {logLines || (
                  <span className="empty-state">{t("logPanel.emptyLog")}</span>
                )}
              </pre>
            </>
          ) : (
            <div className="empty-state">{t("logPanel.noLog")}</div>
          )}
        </div>
      )}
    </div>
  );
}
