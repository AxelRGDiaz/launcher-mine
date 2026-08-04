import { useEffect, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { api } from "@/lib/api";
import { useLauncherConfig } from "@/theme/ThemeProvider";
import type { Account, DeviceCodeInfo } from "@/lib/types";

export function Accounts() {
  const { config } = useLauncherConfig();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [username, setUsername] = useState("");
  const [error, setError] = useState<string | null>(null);

  const [deviceCode, setDeviceCode] = useState<DeviceCodeInfo | null>(null);
  const [msStatus, setMsStatus] = useState<"idle" | "waiting" | "polling">("idle");
  const [skinVariant, setSkinVariant] = useState<Record<string, "classic" | "slim">>({});
  const [skinBusyId, setSkinBusyId] = useState<string | null>(null);

  async function refresh() {
    setAccounts(await api.accounts.list());
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function handleAdd() {
    setError(null);
    try {
      await api.accounts.add(username.trim());
      setUsername("");
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRemove(id: string) {
    await api.accounts.remove(id);
    await refresh();
  }

  async function handleSkinChange(accountId: string, file: File) {
    setError(null);
    setSkinBusyId(accountId);
    try {
      const buffer = await file.arrayBuffer();
      const bytes = Array.from(new Uint8Array(buffer));
      const variant = skinVariant[accountId] ?? "classic";
      await api.accounts.changeSkin(accountId, bytes, variant);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setSkinBusyId(null);
    }
  }

  async function handleMicrosoftLogin() {
    setError(null);
    setMsStatus("waiting");
    try {
      const info = await api.accounts.startMicrosoftLogin();
      setDeviceCode(info);
      void openUrl(info.verificationUri);
      setMsStatus("polling");
      await api.accounts.completeMicrosoftLogin();
      setDeviceCode(null);
      setMsStatus("idle");
      await refresh();
    } catch (err) {
      setError(String(err));
      setDeviceCode(null);
      setMsStatus("idle");
    }
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <div>
        <h1 className="text-lg font-semibold text-text">Cuentas</h1>
        <p className="text-sm text-text-muted">
          Cuentas de Microsoft para jugar en línea, o cuentas sin conexión para pruebas en un solo jugador.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-red-900/50 bg-red-950/30 px-3 py-2 text-sm text-red-300">{error}</div>
      )}

      {/* Microsoft */}
      <section className="flex flex-col gap-3 rounded-lg border border-border bg-surface-raised p-4">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Cuenta Microsoft</h2>
        {!config?.microsoftClientId && (
          <p className="text-xs text-text-muted">
            No hay un <code>microsoftClientId</code> configurado — ver el README para registrar tu propia app en
            Microsoft Entra (requisito de Microsoft, gratis).
          </p>
        )}
        {config?.microsoftClientId && msStatus === "idle" && (
          <button
            onClick={handleMicrosoftLogin}
            className="w-fit rounded-md bg-primary px-4 py-2 text-sm font-medium text-white"
          >
            Iniciar sesión con Microsoft
          </button>
        )}
        {deviceCode && (
          <div className="flex flex-col gap-2 rounded-md border border-border bg-surface-sunken p-3">
            <p className="text-sm text-text">
              Se abrió <span className="text-primary">{deviceCode.verificationUri}</span> en tu navegador — ingresa
              este código:
            </p>
            <p className="text-center text-2xl font-bold tracking-widest text-text">{deviceCode.userCode}</p>
            <p className="text-xs text-text-muted">Esperando a que termines de iniciar sesión ahí…</p>
          </div>
        )}
        {msStatus === "waiting" && !deviceCode && <p className="text-xs text-text-muted">Conectando con Microsoft…</p>}
      </section>

      {/* Offline */}
      <section className="flex flex-col gap-3 rounded-lg border border-border bg-surface-raised p-4">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-text-muted">Cuenta sin conexión</h2>
        <div className="flex gap-2">
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="Nombre de usuario (3-16 caracteres)"
            className="input flex-1"
          />
          <button onClick={handleAdd} className="rounded-md border border-border px-4 py-2 text-sm text-text hover:bg-surface-sunken">
            Añadir
          </button>
        </div>
      </section>

      <div className="flex flex-col gap-2">
        {accounts.length === 0 && (
          <p className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-text-muted">
            No hay cuentas todavía.
          </p>
        )}
        {accounts.map((account) => (
          <div
            key={account.id}
            className="flex items-center gap-3 rounded-lg border border-border bg-surface-raised px-4 py-3"
          >
            {account.skinUrl ? (
              <img src={account.skinUrl} alt="" className="h-8 w-8 shrink-0 rounded" />
            ) : (
              <div className="h-8 w-8 shrink-0 rounded bg-surface-sunken" />
            )}
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="truncate text-sm font-medium text-text">{account.username}</p>
                <span className="shrink-0 rounded bg-surface-sunken px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-text-muted">
                  {account.kind === "microsoft" ? "Microsoft" : "Sin conexión"}
                </span>
              </div>
              <p className="truncate text-xs text-text-muted">{account.uuid}</p>
            </div>
            {account.kind === "microsoft" && (
              <div className="flex shrink-0 items-center gap-2">
                <select
                  value={skinVariant[account.id] ?? "classic"}
                  onChange={(e) =>
                    setSkinVariant((prev) => ({ ...prev, [account.id]: e.target.value as "classic" | "slim" }))
                  }
                  className="rounded-md border border-border bg-surface-sunken px-2 py-1.5 text-xs text-text"
                >
                  <option value="classic">Steve</option>
                  <option value="slim">Alex</option>
                </select>
                <label className="cursor-pointer rounded-md border border-border px-2 py-1.5 text-xs text-text hover:bg-surface-sunken">
                  {skinBusyId === account.id ? "Subiendo…" : "Cambiar skin"}
                  <input
                    type="file"
                    accept="image/png"
                    disabled={skinBusyId === account.id}
                    className="hidden"
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      e.target.value = "";
                      if (file) void handleSkinChange(account.id, file);
                    }}
                  />
                </label>
              </div>
            )}
            <button
              onClick={() => handleRemove(account.id)}
              className="shrink-0 rounded-md px-2 py-1.5 text-xs text-text-muted hover:bg-surface-sunken hover:text-red-400"
            >
              Cerrar sesión
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
