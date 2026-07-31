import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Account } from "@/lib/types";

export function Accounts() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [username, setUsername] = useState("");
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <div>
        <h1 className="text-lg font-semibold text-text">Cuentas</h1>
        <p className="text-sm text-text-muted">
          Cuentas sin conexión para pruebas en un solo jugador. La autenticación oficial de
          Microsoft/Xbox llegará en una fase posterior (requiere registrar una app en Azure AD).
        </p>
      </div>

      <div className="rounded-lg border border-amber-900/40 bg-amber-950/20 px-3 py-2 text-xs text-amber-300">
        Modo sin conexión: válido solo para un jugador con una copia legítima ya instalada. No
        funciona para multijugador ni Realms, ya que esos los valida el propio servidor de Mojang.
      </div>

      {error && (
        <div className="rounded-md border border-red-900/50 bg-red-950/30 px-3 py-2 text-sm text-red-300">{error}</div>
      )}

      <div className="flex gap-2">
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="Nombre de usuario (3-16 caracteres)"
          className="flex-1 rounded-md border border-border bg-surface-raised px-3 py-2 text-sm text-text outline-none focus:border-primary"
        />
        <button onClick={handleAdd} className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-white">
          Añadir cuenta
        </button>
      </div>

      <div className="flex flex-col gap-2">
        {accounts.length === 0 && (
          <p className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-text-muted">
            No hay cuentas todavía.
          </p>
        )}
        {accounts.map((account) => (
          <div
            key={account.id}
            className="flex items-center justify-between rounded-lg border border-border bg-surface-raised px-4 py-3"
          >
            <div>
              <p className="text-sm font-medium text-text">{account.username}</p>
              <p className="text-xs text-text-muted">{account.uuid}</p>
            </div>
            <button
              onClick={() => handleRemove(account.id)}
              className="rounded-md px-2 py-1.5 text-xs text-text-muted hover:bg-surface-sunken hover:text-red-400"
            >
              Cerrar sesión
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
