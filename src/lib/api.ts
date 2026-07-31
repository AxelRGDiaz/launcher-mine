import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Account,
  DownloadProgress,
  Instance,
  JavaInstallation,
  LauncherConfig,
  ModEntry,
  VersionEntry,
} from "./types";

/** Wrapper tipado sobre `invoke()` — un único lugar que conoce los nombres
 * de comando de Tauri, para que el resto de la UI trabaje con funciones
 * normales de TypeScript. */
export const api = {
  config: {
    get: () => invoke<LauncherConfig>("get_config"),
    save: (config: LauncherConfig) => invoke<LauncherConfig>("save_config", { config }),
    reset: () => invoke<LauncherConfig>("reset_config"),
  },

  system: {
    memoryMb: () => invoke<number>("system_memory_mb"),
  },

  java: {
    detect: () => invoke<JavaInstallation[]>("detect_java"),
    install: (major: number) => invoke<JavaInstallation>("install_java", { major }),
  },

  versions: {
    list: () => invoke<VersionEntry[]>("list_minecraft_versions"),
  },

  instances: {
    list: () => invoke<Instance[]>("list_instances"),
    create: (name: string, minecraftVersion: string) =>
      invoke<Instance>("create_instance", { name, minecraftVersion }),
    update: (instance: Instance) => invoke<void>("update_instance", { instance }),
    delete: (instanceId: string) => invoke<void>("delete_instance", { instanceId }),
    isVersionInstalled: (minecraftVersion: string) =>
      invoke<boolean>("is_version_installed", { minecraftVersion }),
    install: (instanceId: string) => invoke<void>("install_instance", { instanceId }),
    launch: (instanceId: string, accountId: string) =>
      invoke<void>("launch_instance", { instanceId, accountId }),
    isRunning: (instanceId: string) => invoke<boolean>("is_instance_running", { instanceId }),
  },

  mods: {
    list: (instanceId: string) => invoke<ModEntry[]>("list_mods", { instanceId }),
    toggle: (instanceId: string, fileName: string, enable: boolean) =>
      invoke<void>("toggle_mod", { instanceId, fileName, enable }),
  },

  accounts: {
    list: () => invoke<Account[]>("list_accounts"),
    add: (username: string) => invoke<Account>("add_account", { username }),
    remove: (accountId: string) => invoke<void>("remove_account", { accountId }),
  },

  events: {
    onInstallProgress: (cb: (progress: DownloadProgress) => void): Promise<UnlistenFn> =>
      listen<DownloadProgress>("install-progress", (e) => cb(e.payload)),
    onJavaInstallProgress: (cb: (progress: DownloadProgress) => void): Promise<UnlistenFn> =>
      listen<DownloadProgress>("java-install-progress", (e) => cb(e.payload)),
    onInstanceExited: (cb: (instanceId: string) => void): Promise<UnlistenFn> =>
      listen<string>("instance-exited", (e) => cb(e.payload)),
    onInstanceLog: (instanceId: string, cb: (line: string) => void): Promise<UnlistenFn> =>
      listen<string>(`instance-log:${instanceId}`, (e) => cb(e.payload)),
  },
};
