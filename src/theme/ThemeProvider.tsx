import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "@/lib/api";
import type { BrandingImages, LauncherConfig } from "@/lib/types";

/**
 * Paletas base por tema. Deben mantenerse en sincronía con
 * `config/themes/dark.json` y `config/themes/light.json` — viven duplicadas
 * aquí (en vez de leerse por IPC) para que el primer render no dependa de
 * una vuelta extra al backend. Si se añade un editor de temas custom, este
 * es el punto a extender para cargar JSON arbitrario del usuario.
 */
const THEME_PALETTES: Record<"dark" | "light", Record<string, string>> = {
  dark: {
    surface: "#121212",
    surfaceRaised: "#1c1c1f",
    surfaceSunken: "#0a0a0b",
    border: "#2a2a2e",
    text: "#f2f2f3",
    textMuted: "#9a9aa2",
  },
  light: {
    surface: "#f5f5f7",
    surfaceRaised: "#ffffff",
    surfaceSunken: "#e8e8ea",
    border: "#d8d8dc",
    text: "#1b1b1f",
    textMuted: "#6a6a72",
  },
};

interface ThemeContextValue {
  config: LauncherConfig | null;
  images: BrandingImages | null;
  loading: boolean;
  reload: () => Promise<void>;
  updateConfig: (next: LauncherConfig) => Promise<void>;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function applyCssVariables(config: LauncherConfig) {
  const palette = THEME_PALETTES[config.theme === "light" ? "light" : "dark"];
  const root = document.documentElement;
  root.style.setProperty("--color-primary", config.primaryColor);
  root.style.setProperty("--color-surface", palette.surface);
  root.style.setProperty("--color-surface-raised", palette.surfaceRaised);
  root.style.setProperty("--color-surface-sunken", palette.surfaceSunken);
  root.style.setProperty("--color-border", palette.border);
  root.style.setProperty("--color-text", palette.text);
  root.style.setProperty("--color-text-muted", palette.textMuted);
  root.classList.toggle("dark", config.theme !== "light");
  document.title = config.launcherName;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<LauncherConfig | null>(null);
  const [images, setImages] = useState<BrandingImages | null>(null);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const loaded = await api.config.get();
      setConfig(loaded);
      applyCssVariables(loaded);
    } finally {
      setLoading(false);
    }
  }, []);

  const updateConfig = useCallback(async (next: LauncherConfig) => {
    const saved = await api.config.save(next);
    setConfig(saved);
    applyCssVariables(saved);
  }, []);

  useEffect(() => {
    void reload();
    // Las imágenes están embebidas en el binario, no cambian en runtime —
    // basta con pedirlas una vez.
    void api.branding.images().then(setImages);
  }, [reload]);

  const value = useMemo(
    () => ({ config, images, loading, reload, updateConfig }),
    [config, images, loading, reload, updateConfig],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useLauncherConfig() {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useLauncherConfig debe usarse dentro de <ThemeProvider>");
  }
  return ctx;
}
