import { useEffect, useMemo, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/api";
import type { Account, DownloadProgress, Instance, LoaderKind, LoaderVersionEntry, VersionEntry } from "@/lib/types";
import { InstanceCard } from "@/components/InstanceCard";
import { ProgressBar } from "@/components/ProgressBar";

const CREATABLE_LOADERS: { value: LoaderKind; label: string }[] = [
  { value: "vanilla", label: "Vanilla" },
  { value: "fabric", label: "Fabric" },
  { value: "quilt", label: "Quilt" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
  { value: "optifine", label: "OptiFine" },
];

export function Instances() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [installedInstances, setInstalledInstances] = useState<Record<string, boolean>>({});
  const [runningIds, setRunningIds] = useState<Record<string, boolean>>({});
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");

  const [busyInstanceId, setBusyInstanceId] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newVersion, setNewVersion] = useState("");
  const [newLoader, setNewLoader] = useState<LoaderKind>("vanilla");
  const [loaderVersions, setLoaderVersions] = useState<LoaderVersionEntry[]>([]);
  const [newLoaderVersion, setNewLoaderVersion] = useState("");
  const [loadingLoaderVersions, setLoadingLoaderVersions] = useState(false);
  const [optifineImports, setOptifineImports] = useState<string[]>([]);
  const [importingOptifine, setImportingOptifine] = useState(false);

  async function refreshInstances() {
    const list = await api.instances.list();
    setInstances(list);
    const entries = await Promise.all(
      list.map(async (i) => [i.id, await api.instances.isInstanceInstalled(i.id)] as const),
    );
    setInstalledInstances(Object.fromEntries(entries));
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

  // Fabric/Quilt/Forge/NeoForge exponen versiones de loader específicas por
  // versión de Minecraft, así que hay que volver a pedirlas cada vez que
  // cambia cualquiera de las dos. OptiFine no tiene API: se maneja aparte
  // (ver el otro useEffect, contra los archivos ya importados).
  useEffect(() => {
    if (newLoader === "vanilla" || newLoader === "optifine" || !newVersion) {
      setLoaderVersions([]);
      setNewLoaderVersion("");
      return;
    }
    setLoadingLoaderVersions(true);
    void api.loaders
      .listVersions(newVersion, newLoader)
      .then((entries) => {
        setLoaderVersions(entries);
        const stable = entries.find((e) => e.stable) ?? entries[0];
        setNewLoaderVersion(stable?.version ?? "");
      })
      .catch((err) => setError(String(err)))
      .finally(() => setLoadingLoaderVersions(false));
  }, [newLoader, newVersion]);

  async function refreshOptifineImports() {
    if (!newVersion) return;
    const imports = await api.optifine.listImports(newVersion);
    setOptifineImports(imports);
    setNewLoaderVersion(imports[0] ?? "");
  }

  useEffect(() => {
    if (newLoader === "optifine") void refreshOptifineImports();
  }, [newLoader, newVersion]);

  async function handleImportOptifine() {
    const selected = await openFileDialog({
      multiple: false,
      filters: [{ name: "OptiFine", extensions: ["jar"] }],
      title: "Selecciona el .jar de OptiFine que descargaste de optifine.net",
    });
    if (!selected || Array.isArray(selected)) return;
    setImportingOptifine(true);
    setError(null);
    try {
      const fileName = await api.optifine.import(selected);
      await refreshOptifineImports();
      setNewLoaderVersion(fileName);
    } catch (err) {
      setError(String(err));
    } finally {
      setImportingOptifine(false);
    }
  }

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
    if (!confirm(`¿Eliminar la versión "${instance.name}"? Esta acción no se puede deshacer.`)) return;
    await api.instances.delete(instance.id);
    await refreshInstances();
  }

  async function handleCreate() {
    if (!newName.trim() || !newVersion) return;
    if (newLoader !== "vanilla" && !newLoaderVersion) return;
    setError(null);
    try {
      await api.instances.create(
        newName.trim(),
        newVersion,
        newLoader,
        newLoader === "vanilla" ? null : newLoaderVersion,
      );
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
          <h1 className="text-lg font-semibold text-text">Versiones</h1>
          <p className="text-sm text-text-muted">Cada versión es un perfil independiente de Minecraft.</p>
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
            Nueva versión
          </button>
        </div>
      </div>

      {error && (
        <div className="rounded-md border border-red-900/50 bg-red-950/30 px-3 py-2 text-sm text-red-300">{error}</div>
      )}

      {showCreate && (
        <div className="rounded-lg border border-border bg-surface-raised p-4">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Nombre de la versión"
              className="input"
            />
            <select value={newLoader} onChange={(e) => setNewLoader(e.target.value as LoaderKind)} className="input">
              {CREATABLE_LOADERS.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
            <select value={newVersion} onChange={(e) => setNewVersion(e.target.value)} className="input">
              {versions.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.id} {v.type !== "release" ? `(${v.type})` : ""}
                </option>
              ))}
            </select>
            {newLoader !== "vanilla" && newLoader !== "optifine" && (
              <select
                value={newLoaderVersion}
                onChange={(e) => setNewLoaderVersion(e.target.value)}
                disabled={loadingLoaderVersions || loaderVersions.length === 0}
                className="input"
              >
                {loadingLoaderVersions && <option>Cargando versiones…</option>}
                {!loadingLoaderVersions && loaderVersions.length === 0 && (
                  <option>Sin versiones disponibles para {newVersion}</option>
                )}
                {loaderVersions.map((l) => (
                  <option key={l.version} value={l.version}>
                    {l.version} {l.stable ? "" : "(inestable)"}
                  </option>
                ))}
              </select>
            )}
            {newLoader === "optifine" && (
              <div className="flex gap-2 sm:col-span-2">
                <select
                  value={newLoaderVersion}
                  onChange={(e) => setNewLoaderVersion(e.target.value)}
                  disabled={optifineImports.length === 0}
                  className="input flex-1"
                >
                  {optifineImports.length === 0 && <option>Ningún OptiFine importado para {newVersion} todavía</option>}
                  {optifineImports.map((fileName) => (
                    <option key={fileName} value={fileName}>
                      {fileName}
                    </option>
                  ))}
                </select>
                <button
                  onClick={handleImportOptifine}
                  disabled={importingOptifine}
                  className="shrink-0 rounded-md border border-border px-3 py-2 text-sm text-text hover:bg-surface-sunken disabled:opacity-50"
                >
                  {importingOptifine ? "Importando…" : "Importar .jar…"}
                </button>
              </div>
            )}
          </div>
          {newLoader === "optifine" && (
            <p className="mt-2 text-xs text-text-muted">
              Descarga el <code>.jar</code> de OptiFine para {newVersion} desde{" "}
              <span className="text-text">optifine.net</span> tú mismo, luego impórtalo aquí. Solo hace falta una vez
              por versión.
            </p>
          )}
          <button
            onClick={handleCreate}
            className="mt-3 rounded-md bg-primary px-4 py-2 text-sm font-medium text-white"
          >
            Crear
          </button>
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
            Todavía no hay versiones. Crea una para empezar a jugar.
          </p>
        )}
        {instances.map((instance) => (
          <InstanceCard
            key={instance.id}
            instance={instance}
            installed={!!installedInstances[instance.id]}
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
