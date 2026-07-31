import type { LauncherConfig } from "@/lib/types";
import { useLauncherConfig } from "@/theme/ThemeProvider";

export type ScreenId = "home" | "instances" | "mods" | "accounts" | "settings" | "about";

const NAV_ITEMS: { id: ScreenId; label: string }[] = [
  { id: "home", label: "Inicio" },
  { id: "instances", label: "Versiones" },
  { id: "mods", label: "Mods" },
  { id: "accounts", label: "Cuentas" },
  { id: "settings", label: "Configuración" },
  { id: "about", label: "Acerca de" },
];

interface SidebarProps {
  active: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  config: LauncherConfig;
}

export function Sidebar({ active, onNavigate, config }: SidebarProps) {
  const { images } = useLauncherConfig();
  const initials = config.launcherName
    .split(/\s+/)
    .map((word) => word[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();

  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r border-border bg-surface-sunken">
      <div className="flex items-center gap-3 px-4 py-5">
        {images?.logo ? (
          <img src={images.logo} alt="" className="h-9 w-9 shrink-0 rounded-lg object-cover" />
        ) : (
          <div
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white"
            style={{ backgroundColor: config.primaryColor }}
          >
            {initials || "ML"}
          </div>
        )}
        <span className="truncate text-sm font-semibold text-text">{config.launcherName}</span>
      </div>

      <nav className="flex flex-1 flex-col gap-1 px-2">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            onClick={() => onNavigate(item.id)}
            className={`rounded-md px-3 py-2 text-left text-sm transition-colors ${
              active === item.id
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised/60 hover:text-text"
            }`}
            style={active === item.id ? { boxShadow: `inset 2px 0 0 ${config.primaryColor}` } : undefined}
          >
            {item.label}
          </button>
        ))}
      </nav>
    </aside>
  );
}
