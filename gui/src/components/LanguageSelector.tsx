import { useContext } from "react";
import { useTranslation, LanguageContext, AVAILABLE_LANGS } from "../i18n";

export default function LanguageSelector() {
  const { t } = useTranslation();
  const { lang, setLang } = useContext(LanguageContext);

  return (
    <div className="settings-tile">
      <h3>{t("language.header")}</h3>
      <p className="tile-desc">{t("language.desc")}</p>
      <select
        className="lang-select"
        value={lang}
        onChange={(e) => setLang(e.target.value as typeof lang)}
        style={{
          padding: "6px 10px",
          fontSize: 13,
          borderRadius: 6,
          border: "1px solid var(--border)",
          background: "var(--bg-card)",
          color: "var(--text-primary)",
          fontFamily: "var(--font-sans)",
          cursor: "pointer",
          minWidth: 220,
        }}
      >
        {AVAILABLE_LANGS.map((l) => (
          <option key={l.code} value={l.code}>
            {l.nativeName}
          </option>
        ))}
      </select>
    </div>
  );
}
