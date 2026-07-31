import { useEffect } from "react";
import { useUpdater } from "@/lib/useUpdater";

export function UpdateBanner() {
  const { status, update, progress, error, checkForUpdate, installAndRelaunch } = useUpdater();

  useEffect(() => {
    void checkForUpdate();
  }, [checkForUpdate]);

  if (status === "idle" || status === "checking" || status === "up-to-date") return null;

  return (
    <div className="flex items-center justify-between gap-3 border-b border-border bg-surface-raised px-4 py-2 text-sm">
      {status === "available" && (
        <>
          <span className="text-text">
            Hay una nueva versión disponible{update ? ` (${update.version})` : ""}.
          </span>
          <button
            onClick={() => void installAndRelaunch()}
            className="shrink-0 rounded-md bg-primary px-3 py-1 text-xs font-medium text-white"
          >
            Actualizar y reiniciar
          </button>
        </>
      )}
      {status === "downloading" && <span className="text-text-muted">Descargando actualización… {progress}%</span>}
      {status === "ready" && <span className="text-text-muted">Reiniciando…</span>}
      {status === "error" && <span className="text-red-400">No se pudo comprobar actualizaciones: {error}</span>}
    </div>
  );
}
