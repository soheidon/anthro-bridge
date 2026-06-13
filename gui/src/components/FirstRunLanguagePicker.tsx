import { useContext, useState } from "react";
import { LanguageContext } from "../i18n";
import { AVAILABLE_LANGS } from "../i18n";
import type { Lang } from "../i18n";

interface FirstRunLanguagePickerProps {
  onDone: () => void;
}

export default function FirstRunLanguagePicker({ onDone }: FirstRunLanguagePickerProps) {
  const { setLang } = useContext(LanguageContext);
  const [selected, setSelected] = useState<Lang | null>(null);

  const handleSelect = (code: Lang) => {
    setSelected(code);
    setLang(code);
    // Brief delay so user sees their selection highlighted
    setTimeout(onDone, 400);
  };

  return (
    <div style={{
      position: "fixed",
      inset: 0,
      background: "var(--bg-primary, #0f172a)",
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      zIndex: 9999,
      fontFamily: "var(--font-sans, 'Segoe UI', sans-serif)",
    }}>
      <h1 style={{
        fontSize: 22,
        fontWeight: 700,
        color: "var(--text-primary, #f1f5f9)",
        marginBottom: 8,
        textAlign: "center",
      }}>
        Terra Bridge
      </h1>
      <p style={{
        fontSize: 14,
        color: "var(--text-muted, #94a3b8)",
        marginBottom: 28,
        textAlign: "center",
      }}>
        Select your language / 言語を選択 / 选择语言 / 언어 선택 / Choisir la langue
      </p>
      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(3, 1fr)",
        gap: 12,
        maxWidth: 480,
      }}>
        {AVAILABLE_LANGS.map((l) => (
          <button
            key={l.code}
            onClick={() => handleSelect(l.code as Lang)}
            disabled={selected !== null}
            style={{
              padding: "14px 20px",
              fontSize: 15,
              fontWeight: 600,
              border: selected === l.code
                ? "2px solid var(--accent, #3b82f6)"
                : "1px solid var(--border, #334155)",
              borderRadius: 8,
              background: selected === l.code
                ? "rgba(59,130,246,0.15)"
                : "var(--bg-card, #1e293b)",
              color: "var(--text-primary, #f1f5f9)",
              cursor: selected !== null ? "default" : "pointer",
              opacity: selected !== null && selected !== l.code ? 0.4 : 1,
              transition: "all 0.15s",
            }}
          >
            {l.nativeName}
          </button>
        ))}
      </div>
    </div>
  );
}
