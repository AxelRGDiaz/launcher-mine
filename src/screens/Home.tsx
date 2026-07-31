import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { useLauncherConfig } from "@/theme/ThemeProvider";
import type { Account, Instance } from "@/lib/types";
import type { ScreenId } from "@/components/Sidebar";

interface HomeProps {
  onNavigate: (screen: ScreenId) => void;
}

export function Home({ onNavigate }: HomeProps) {
  const { config, images } = useLauncherConfig();
  const [instances, setInstances] = useState<Instance[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [installed, setInstalled] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.instances.list().then(setInstances);
    void api.accounts.list().then(setAccounts);
  }, []);

  const primaryInstance =
    [...instances].sort((a, b) => (b.lastPlayed ?? "").localeCompare(a.lastPlayed ?? ""))[0] ?? null;

  useEffect(() => {
    if (primaryInstance) {
      void api.instances.isInstanceInstalled(primaryInstance.id).then(setInstalled);
    }
  }, [primaryInstance?.id]);

  async function handlePlay() {
    if (!primaryInstance) {
      onNavigate("instances");
      return;
    }
    if (accounts.length === 0) {
      onNavigate("accounts");
      return;
    }
    setError(null);
    setLaunching(true);
    try {
      if (!installed) {
        await api.instances.install(primaryInstance.id);
        setInstalled(true);
      }
      await api.instances.launch(primaryInstance.id, accounts[0].id);
    } catch (err) {
      setError(String(err));
    } finally {
      setLaunching(false);
    }
  }

  return (
    <div className="relative flex h-full flex-col items-center justify-center gap-6 p-6 text-center">
      {images?.banner && (
        <>
          <div
            className="absolute inset-0 -z-10 bg-cover bg-center"
            style={{ backgroundImage: `url(${images.banner})` }}
          />
          <div className="absolute inset-0 -z-10 bg-black/55" />
        </>
      )}
      <div className="relative">
        <h1 className="text-2xl font-semibold text-white drop-shadow">{config?.welcomeText}</h1>
        {primaryInstance && (
          <p className="mt-2 text-sm text-white/80 drop-shadow">
            Última versión: <span className="text-white">{primaryInstance.name}</span> ({primaryInstance.minecraftVersion})
          </p>
        )}
      </div>

      <button
        onClick={handlePlay}
        disabled={launching}
        className="rounded-xl bg-primary px-10 py-4 text-lg font-semibold text-white shadow-lg transition-transform hover:scale-[1.02] disabled:opacity-60"
      >
        {launching ? "Preparando…" : primaryInstance ? "Jugar" : "Crear tu primera versión"}
      </button>

      {error && <p className="max-w-md text-sm text-red-400">{error}</p>}

      {accounts.length === 0 && (
        <p className="text-xs text-text-muted">
          No tienes ninguna cuenta añadida.{" "}
          <button className="underline" onClick={() => onNavigate("accounts")}>
            Añade una en Cuentas
          </button>
          .
        </p>
      )}
    </div>
  );
}
