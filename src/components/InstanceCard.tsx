import type { Instance } from "@/lib/types";

interface InstanceCardProps {
  instance: Instance;
  installed: boolean;
  running: boolean;
  busy: boolean;
  onPlay: () => void;
  onInstall: () => void;
  onDelete: () => void;
}

function formatPlaytime(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (hours === 0 && minutes === 0) return "Sin jugar todavía";
  if (hours === 0) return `${minutes} min jugados`;
  return `${hours} h ${minutes} min jugados`;
}

export function InstanceCard({ instance, installed, running, busy, onPlay, onInstall, onDelete }: InstanceCardProps) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-border bg-surface-raised px-4 py-3">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate font-medium text-text">{instance.name}</span>
          <span className="shrink-0 rounded bg-surface-sunken px-1.5 py-0.5 text-[11px] uppercase tracking-wide text-text-muted">
            {instance.loader}
          </span>
        </div>
        <p className="mt-0.5 text-xs text-text-muted">
          Minecraft {instance.minecraftVersion} · {formatPlaytime(instance.totalPlaytimeSecs)}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <button
          onClick={onDelete}
          className="rounded-md px-2 py-1.5 text-xs text-text-muted hover:bg-surface-sunken hover:text-red-400"
        >
          Eliminar
        </button>
        {installed ? (
          <button
            onClick={onPlay}
            disabled={busy || running}
            className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
          >
            {running ? "Jugando…" : busy ? "Iniciando…" : "Jugar"}
          </button>
        ) : (
          <button
            onClick={onInstall}
            disabled={busy}
            className="rounded-md border border-border px-3 py-1.5 text-sm font-medium text-text hover:bg-surface-sunken disabled:opacity-50"
          >
            {busy ? "Instalando…" : "Instalar"}
          </button>
        )}
      </div>
    </div>
  );
}
