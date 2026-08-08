import { useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "@/lib/api";
import { applyCssVariables, useLauncherConfig } from "@/theme/ThemeProvider";
import type { JavaInstallation } from "@/lib/types";
import { RamSlider } from "@/components/RamSlider";
import { ProgressBar } from "@/components/ProgressBar";

const JAVA_MAJORS: { major: number; hint: string }[] = [
  { major: 8, hint: "Minecraft 1.16 y anteriores" },
  { major: 17, hint: "Minecraft 1.17 – 1.20.4" },
  { major: 21, hint: "Minecraft 1.20.5 – 26.1" },
  { major: 25, hint: "Minecraft 26.2 y superior" },
];

export function Settings() {
  const { config, updateConfig, reload } = useLauncherConfig();
  const [draft, setDraft] = useState(config);
  const [systemMemoryMb, setSystemMemoryMb] = useState(8192);
  const [javaInstalls, setJavaInstalls] = useState<JavaInstallation[]>([]);
  const [installingMajor, setInstallingMajor] = useState<number | null>(null);
  const [javaProgress, setJavaProgress] = useState<{ downloaded: number; total: number | null } | null>(null);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [customMajor, setCustomMajor] = useState("");

  useEffect(() => setDraft(config), [config]);

  // Vista previa en vivo: el tema/color se ven al elegirlos, sin esperar a
  // "Guardar cambios". Si se sale de esta pantalla sin guardar, se revierte
  // a lo que de verdad está persistido (config), para no dejar la UI en un
  // estado a medias.
  useEffect(() => {
    if (draft) applyCssVariables(draft);
    return () => {
      if (config) applyCssVariables(config);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft?.theme, draft?.primaryColor]);

  useEffect(() => {
    void api.system.memoryMb().then(setSystemMemoryMb);
    void refreshJava();
    const unlisten = api.events.onJavaInstallProgress((p) =>
      setJavaProgress({ downloaded: p.downloadedBytes, total: p.totalBytes }),
    );
    return () => void unlisten.then((f) => f());
  }, []);

  async function refreshJava() {
    setJavaInstalls(await api.java.detect());
  }

  async function handleSave() {
    if (!draft) return;
    setError(null);
    try {
      await updateConfig(draft);
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleReset() {
    if (!confirm("¿Restaurar toda la configuración a los valores por defecto?")) return;
    await api.config.reset();
    await reload();
  }

  async function handleInstallJava(major: number) {
    setInstallingMajor(major);
    setJavaProgress(null);
    setError(null);
    try {
      await api.java.install(major);
      await refreshJava();
    } catch (err) {
      setError(String(err));
    } finally {
      setInstallingMajor(null);
      setJavaProgress(null);
    }
  }

  const javaProgressRatio = useMemo(() => {
    if (!javaProgress || !javaProgress.total) return null;
    return javaProgress.downloaded / javaProgress.total;
  }, [javaProgress]);

  if (!draft) return null;

  return (
    <div className="flex h-full flex-col gap-8 overflow-y-auto p-6">
      <div>
        <h1 className="text-lg font-semibold text-text">Configuración</h1>
        <p className="text-sm text-text-muted">
          {draft.launcherName} · el nombre, logo y colores se fijan antes de compilar (ver{" "}
          <code>config/config.default.json</code> y <code>tauri.conf.json</code>), no se editan aquí.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-red-900/50 bg-red-950/30 px-3 py-2 text-sm text-red-300">{error}</div>
      )}

      {/* Preferencias de uso — a diferencia de la marca (nombre/logo/color),
          esto sí tiene sentido que lo cambie quien usa el launcher ya compilado. */}
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Preferencias</h2>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Tema">
            <select
              value={draft.theme}
              onChange={(e) => setDraft({ ...draft, theme: e.target.value as "dark" | "light" })}
              className="input"
            >
              <option value="dark">Oscuro</option>
              <option value="light">Claro</option>
            </select>
          </Field>
          <Field label="Mostrar snapshots">
            <label className="flex h-9 items-center gap-2 text-sm text-text">
              <input
                type="checkbox"
                checked={draft.showSnapshots}
                onChange={(e) => setDraft({ ...draft, showSnapshots: e.target.checked })}
                className="h-4 w-4 accent-[var(--color-primary)]"
              />
              Incluir snapshots en la lista de versiones
            </label>
          </Field>
        </div>
      </section>

      {/* Rendimiento */}
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Rendimiento</h2>
        <p className="text-xs text-text-muted">
          Memoria del sistema detectada: {(systemMemoryMb / 1024).toFixed(1)} GB. Estos valores son el
          default para versiones nuevas; cada versión puede sobreescribirlos.
        </p>
        <RamSlider
          label="RAM mínima"
          valueMb={draft.defaultMinRamMb}
          minMb={512}
          maxMb={systemMemoryMb}
          onChange={(v) => setDraft({ ...draft, defaultMinRamMb: v })}
        />
        <RamSlider
          label="RAM máxima"
          valueMb={draft.defaultMaxRamMb}
          minMb={draft.defaultMinRamMb}
          maxMb={systemMemoryMb}
          onChange={(v) => setDraft({ ...draft, defaultMaxRamMb: v })}
          warnAboveMb={Math.round(systemMemoryMb * 0.75)}
        />
      </section>

      {/* Servidor por defecto */}
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Servidor por defecto</h2>
        <p className="text-xs text-text-muted">
          Se agrega automáticamente a la lista de multijugador de cada versión nueva. Deja el nombre vacío para no
          agregar ninguno.
        </p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Nombre del servidor">
            <input
              value={draft.defaultServerName ?? ""}
              onChange={(e) => setDraft({ ...draft, defaultServerName: e.target.value || null })}
              placeholder="MUNDO PIKIPIKI"
              className="input"
            />
          </Field>
          <Field label="Dirección">
            <input
              value={draft.defaultServerAddress ?? ""}
              onChange={(e) => setDraft({ ...draft, defaultServerAddress: e.target.value || null })}
              placeholder="pikipiki.axel-diaz.com"
              className="input"
            />
          </Field>
          <Field label="Fondo del menú del juego" full>
            <label className="flex h-9 items-center gap-2 text-sm text-text">
              <input
                type="checkbox"
                checked={draft.applyTitleScreenPack}
                onChange={(e) => setDraft({ ...draft, applyTitleScreenPack: e.target.checked })}
                className="h-4 w-4 accent-[var(--color-primary)]"
              />
              Reemplazar el panorama de la pantalla de título del juego por el banner configurado
            </label>
          </Field>
          <Field label="Texto junto a la versión en el menú del juego">
            <input
              value={draft.versionTypeLabel}
              onChange={(e) => setDraft({ ...draft, versionTypeLabel: e.target.value })}
              placeholder="PikiPiki"
              className="input"
            />
          </Field>
        </div>
      </section>

      {/* Java */}
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Java</h2>
        <div className="flex flex-col gap-2">
          {javaInstalls.length === 0 && (
            <p className="text-sm text-text-muted">No se detectó ningún Java compatible en el sistema.</p>
          )}
          {javaInstalls.map((j) => (
            <div
              key={j.path}
              className="flex items-center justify-between rounded-md border border-border bg-surface-raised px-3 py-2 text-sm"
            >
              <span className="truncate text-text-muted">{j.path}</span>
              <span className="shrink-0 text-text">
                Java {j.majorVersion} · {j.arch} {j.managedByLauncher && "· gestionado por el launcher"}
              </span>
            </div>
          ))}
        </div>
        <div className="flex flex-wrap gap-3">
          {JAVA_MAJORS.map(({ major, hint }) => (
            <div key={major} className="flex flex-col items-start gap-1">
              <button
                onClick={() => handleInstallJava(major)}
                disabled={installingMajor !== null}
                className="rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-raised disabled:opacity-50"
              >
                {installingMajor === major ? "Instalando…" : `Instalar Java ${major} (Temurin)`}
              </button>
              <span className="text-xs text-text-muted">{hint}</span>
            </div>
          ))}
        </div>
        <div className="flex items-end gap-2">
          <Field label="Otra versión (si Minecraft pide un Java más nuevo que no está arriba)">
            <input
              type="number"
              min={8}
              value={customMajor}
              onChange={(e) => setCustomMajor(e.target.value)}
              placeholder="ej. 25"
              className="input w-32"
            />
          </Field>
          <button
            onClick={() => {
              const major = parseInt(customMajor, 10);
              if (Number.isFinite(major) && major >= 8) void handleInstallJava(major);
            }}
            disabled={installingMajor !== null || !customMajor}
            className="rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-raised disabled:opacity-50"
          >
            Instalar
          </button>
        </div>
        {installingMajor !== null && (
          <ProgressBar
            ratio={javaProgressRatio}
            label={`Descargando runtime Java ${installingMajor}…`}
          />
        )}
      </section>

      <div className="flex items-center gap-3 border-t border-border pt-4">
        <button onClick={handleSave} className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-white">
          {saved ? "Guardado ✓" : "Guardar cambios"}
        </button>
        <button onClick={handleReset} className="rounded-md px-4 py-2 text-sm text-text-muted hover:text-red-400">
          Restaurar valores por defecto
        </button>
      </div>
    </div>
  );
}

function Field({ label, children, full }: { label: string; children: ReactNode; full?: boolean }) {
  return (
    <div className={full ? "sm:col-span-2" : undefined}>
      <label className="mb-1 block text-xs text-text-muted">{label}</label>
      {children}
    </div>
  );
}
