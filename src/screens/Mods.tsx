import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Instance, ModEntry } from "@/lib/types";

export function Mods() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [mods, setMods] = useState<ModEntry[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    void api.instances.list().then((list) => {
      setInstances(list);
      if (list.length > 0) setSelectedId(list[0].id);
    });
  }, []);

  async function refreshMods(instanceId: string) {
    setLoading(true);
    try {
      setMods(await api.mods.list(instanceId));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (selectedId) void refreshMods(selectedId);
  }, [selectedId]);

  async function handleToggle(mod: ModEntry) {
    await api.mods.toggle(selectedId, mod.fileName, !mod.enabled);
    await refreshMods(selectedId);
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <div>
        <h1 className="text-lg font-semibold text-text">Mods</h1>
        <p className="text-sm text-text-muted">
          Coloca archivos <code>.jar</code> en la carpeta <code>mods</code> de la instancia y actívalos aquí.
        </p>
      </div>

      {instances.length === 0 ? (
        <p className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-text-muted">
          Crea una instancia primero en la pestaña Instancias.
        </p>
      ) : (
        <>
          <select
            value={selectedId}
            onChange={(e) => setSelectedId(e.target.value)}
            className="w-fit rounded-md border border-border bg-surface-raised px-3 py-2 text-sm text-text"
          >
            {instances.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name}
              </option>
            ))}
          </select>

          <div className="flex flex-col gap-2">
            {loading && <p className="text-sm text-text-muted">Cargando…</p>}
            {!loading && mods.length === 0 && (
              <p className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-text-muted">
                No hay mods instalados en esta instancia todavía.
              </p>
            )}
            {mods.map((mod) => (
              <label
                key={mod.fileName}
                className="flex cursor-pointer items-center justify-between rounded-lg border border-border bg-surface-raised px-4 py-3"
              >
                <span className={`truncate text-sm ${mod.enabled ? "text-text" : "text-text-muted line-through"}`}>
                  {mod.fileName}
                </span>
                <input
                  type="checkbox"
                  checked={mod.enabled}
                  onChange={() => handleToggle(mod)}
                  className="h-4 w-4 accent-[var(--color-primary)]"
                />
              </label>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
