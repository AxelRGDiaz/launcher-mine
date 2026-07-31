import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import type { Account, DownloadProgress, Instance, VersionEntry } from "@/lib/types";
import { InstanceCard } from "@/components/InstanceCard";
import { ProgressBar } from "@/components/ProgressBar";

export function Instances() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [installedVersions, setInstalledVersions] = useState<Record<string, boolean>>({});
  const [runningIds, setRunningIds] = useState<Record<string, boolean>>({});
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");

  const [busyInstanceId, setBusyInstanceId] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newVersion, setNewVersion] = useState("");

  async function refreshInstances() {
    const list = await api.instances.list();
    setInstances(list);
    const uniqueVersions = [...new Set(list.map((i) => i.minecraftVersion))];
    const entries = await Promise.all(
      uniqueVersions.map(async (v) => [v, await api.instances.isVersionInstalled(v)] as const),
    );
    setInstalledVersions(Object.fromEntries(entries));
  }

  useEffect(() => {
    void refreshInstances();
    void api.versions.list().then((v) => {
      setVersions(v);
      if (v.length > 0) setNewVersion(v[0].id);
    });
    void api.accounts.list().then((accs) => {
      setAccounts(accs);
      if (accs.length > 0) setSelectedAccountId(accs[0].id);
    });

    const unlistenProgress = api.events.onInstallProgress((p) => setProgress(p));
    const unlistenExit = api.events.onInstanceExited((id) => {
      setRunningIds((prev) => ({ ...prev, [id]: false }));
    });
    return () => {
      void unlistenProgress.then((f) => f());
      void unlistenExit.then((f) => f());
    };
  }, []);

  async function handleInstall(instance: Instance) {
    setError(null);
    setBusyInstanceId(instance.id);
    setProgress(null);
    try {
      await api.instances.install(instance.id);
      await refreshInstances();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyInstanceId(null);
      setProgress(null);
    }
  }

  async function handlePlay(instance: Instance) {
    if (!selectedAccountId) {
      setError("Añade una cuenta en la pestaña Cuentas antes de jugar.");
      return;
    }
    setError(null);
    setBusyInstanceId(instance.id);
    try {
      await api.instances.launch(instance.id, selectedAccountId);
      setRunningIds((prev) => ({ ...prev, [instance.id]: true }));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyInstanceId(null);
    }
  }

  async function handleDelete(instance: Instance) {
    if (!confirm(`¿Eliminar la instancia "${instance.name}"? Esta acción no se puede deshacer.`)) return;
    await api.instances.delete(instance.id);
    await refreshInstances();
  }

  async function handleCreate() {
    if (!newName.trim() || !newVersion) return;
    setError(null);
    try {
      await api.instances.create(newName.trim(), newVersion);
      setNewName("");
      setShowCreate(false);
      await refreshInstances();
    } catch (err) {
      setError(String(err));
    }
  }

  const progressRatio = useMemo(() => {
    if (!progress) return null;
    if (progress.totalFiles > 0) return progress.completedFiles / progress.totalFiles;
    if (progress.totalBytes) return progress.downloadedBytes / progress.totalBytes;
    return null;
  }, [progress]);

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-text">Instancias</h1>
          <p className="text-sm text-text-muted">Cada instancia es un perfil independiente de Minecraft.</p>
        </div>
        <div className="flex items-center gap-2">
          {accounts.length > 0 && (
            <select
              value={selectedAccountId}
              onChange={(e) => setSelectedAccountId(e.target.value)}
              className="rounded-md border border-border bg-surface-raised px-2 py-1.5 text-sm text-text"
            >
              {accounts.map((acc) => (
                <option key={acc.id} value={acc.id}>
                  {acc.username}
                </option>
              ))}
            </select>
          )}
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white"
          >
            Nueva instancia
          </button>
        </div>
      </div>

      {error && (
        <div className="rounded-md border border-red-900/50 bg-red-950/30 px-3 py-2 text-sm text-red-300">{error}</div>
      )}

      {showCreate && (
        <div className="rounded-lg border border-border bg-surface-raised p-4">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_1fr_auto]">
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Nombre de la instancia"
              className="rounded-md border border-border bg-surface px-3 py-2 text-sm text-text outline-none focus:border-primary"
            />
            <select
              value={newVersion}
              onChange={(e) => setNewVersion(e.target.value)}
              className="rounded-md border border-border bg-surface px-3 py-2 text-sm text-text"
            >
              {versions.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.id} {v.type !== "release" ? `(${v.type})` : ""}
                </option>
              ))}
            </select>
            <button onClick={handleCreate} className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-white">
              Crear
            </button>
          </div>
        </div>
      )}

      {busyInstanceId && progress && (
        <div className="rounded-lg border border-border bg-surface-raised p-3">
          <ProgressBar ratio={progressRatio} label={`${progress.label} (${progress.completedFiles}/${progress.totalFiles || "?"})`} />
        </div>
      )}

      <div className="flex flex-col gap-2">
        {instances.length === 0 && (
          <p className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-text-muted">
            Todavía no hay instancias. Crea una para empezar a jugar.
          </p>
        )}
        {instances.map((instance) => (
          <InstanceCard
            key={instance.id}
            instance={instance}
            installed={!!installedVersions[instance.minecraftVersion]}
            running={!!runningIds[instance.id]}
            busy={busyInstanceId === instance.id}
            onPlay={() => handlePlay(instance)}
            onInstall={() => handleInstall(instance)}
            onDelete={() => handleDelete(instance)}
          />
        ))}
      </div>
    </div>
  );
}
