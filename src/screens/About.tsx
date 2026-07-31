import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-shell";
import { useLauncherConfig } from "@/theme/ThemeProvider";
import { useUpdater } from "@/lib/useUpdater";

export function About() {
  const { config } = useLauncherConfig();
  const { status, update, progress, error, checkForUpdate, installAndRelaunch } = useUpdater();
  const [version, setVersion] = useState("");
  useEffect(() => {
    void getVersion().then(setVersion);
  }, []);
  if (!config) return null;

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <h1 className="text-xl font-semibold text-text">{config.launcherName}</h1>
      <p className="text-sm text-text-muted">Versión {version || "…"} · Tauri + Rust + React</p>

      <button
        onClick={() => void open(config.supportUrl)}
        className="mt-2 text-sm text-primary underline underline-offset-2"
      >
        {config.supportUrl}
      </button>

      <div className="mt-6 max-w-md text-xs text-text-muted">
        <p>Sin anuncios, sin telemetría, sin trackers. Todo lo que este launcher descarga viene de las fuentes oficiales de Mojang/Adoptium.</p>
      </div>

      <div className="mt-6 flex flex-col items-center gap-2">
        {(status === "idle" || status === "up-to-date") && (
          <button
            onClick={() => void checkForUpdate()}
            className="rounded-md border border-border px-3 py-1.5 text-xs text-text hover:bg-surface-sunken"
          >
            Buscar actualizaciones
          </button>
        )}
        {status === "checking" && <p className="text-xs text-text-muted">Buscando actualizaciones…</p>}
        {status === "up-to-date" && <p className="text-xs text-text-muted">Ya tienes la última versión.</p>}
        {status === "available" && (
          <>
            <p className="text-xs text-text">Nueva versión disponible{update ? `: ${update.version}` : ""}.</p>
            <button
              onClick={() => void installAndRelaunch()}
              className="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-white"
            >
              Descargar e instalar
            </button>
          </>
        )}
        {status === "downloading" && <p className="text-xs text-text-muted">Descargando… {progress}%</p>}
        {status === "ready" && <p className="text-xs text-text-muted">Reiniciando…</p>}
        {status === "error" && <p className="text-xs text-red-400">No se pudo comprobar: {error}</p>}
      </div>
    </div>
  );
}
