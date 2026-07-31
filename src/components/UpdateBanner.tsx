import { useEffect } from "react";
import { useUpdater } from "@/lib/useUpdater";

export function UpdateBanner() {
  const { status, update, progress, installAndRelaunch, checkForUpdate } = useUpdater();

  useEffect(() => {
    void checkForUpdate();
    // No mostramos errores de esta revisión automática (p. ej. antes de que
    // exista la primera GitHub Release, el updater falla al buscarla) — el
    // usuario puede revisar manualmente desde Acerca de si quiere ver el
    // detalle del error.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (status !== "available" && status !== "downloading" && status !== "ready") return null;

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
    </div>
  );
}
