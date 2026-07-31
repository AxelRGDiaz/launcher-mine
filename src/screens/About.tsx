import { open } from "@tauri-apps/plugin-shell";
import { useLauncherConfig } from "@/theme/ThemeProvider";

export function About() {
  const { config } = useLauncherConfig();
  if (!config) return null;

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <h1 className="text-xl font-semibold text-text">{config.launcherName}</h1>
      <p className="text-sm text-text-muted">Versión 0.1.0 · Tauri + Rust + React</p>

      <button
        onClick={() => void open(config.supportUrl)}
        className="mt-2 text-sm text-primary underline underline-offset-2"
      >
        {config.supportUrl}
      </button>

      <div className="mt-6 max-w-md text-xs text-text-muted">
        <p>Sin anuncios, sin telemetría, sin trackers. Todo lo que este launcher descarga viene de las fuentes oficiales de Mojang/Adoptium.</p>
      </div>
    </div>
  );
}
