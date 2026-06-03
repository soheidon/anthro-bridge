import { createContext, useState, useEffect, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Lang } from "./translations";

export const LanguageContext = createContext<{
  lang: Lang;
  setLang: (lang: Lang) => void;
}>({
  lang: "en",
  setLang: () => {},
});

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>("en");
  const [loaded, setLoaded] = useState(false);

  // Load persisted language on mount
  useEffect(() => {
    invoke<string>("get_user_language")
      .then((saved) => {
        setLangState(saved as Lang);
        setLoaded(true);
      })
      .catch(() => {
        setLoaded(true);
      });
  }, []);

  const setLang = (newLang: Lang) => {
    setLangState(newLang);
    invoke("set_user_language", { language: newLang }).catch(console.error);
  };

  if (!loaded) {
    // Prevent flash of wrong language
    return <>{children}</>;
  }

  return (
    <LanguageContext.Provider value={{ lang, setLang }}>
      {children}
    </LanguageContext.Provider>
  );
}
