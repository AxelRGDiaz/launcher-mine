import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Account,
  BrandingImages,
  DeviceCodeInfo,
  DownloadProgress,
  Instance,
  JavaInstallation,
  LauncherConfig,
  LoaderKind,
  LoaderVersionEntry,
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

  branding: {
    images: () => invoke<BrandingImages>("get_branding_images"),
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
    create: (name: string, minecraftVersion: string, loader: LoaderKind, loaderVersion: string | null) =>
      invoke<Instance>("create_instance", { name, minecraftVersion, loader, loaderVersion }),
    update: (instance: Instance) => invoke<void>("update_instance", { instance }),
    delete: (instanceId: string) => invoke<void>("delete_instance", { instanceId }),
    isInstanceInstalled: (instanceId: string) => invoke<boolean>("is_instance_installed", { instanceId }),
    install: (instanceId: string) => invoke<void>("install_instance", { instanceId }),
    launch: (instanceId: string, accountId: string) =>
      invoke<void>("launch_instance", { instanceId, accountId }),
    isRunning: (instanceId: string) => invoke<boolean>("is_instance_running", { instanceId }),
  },

  loaders: {
    listVersions: (minecraftVersion: string, loader: LoaderKind) =>
      invoke<LoaderVersionEntry[]>("list_loader_versions", { minecraftVersion, loader }),
  },

  optifine: {
    /** `sourcePath` viene del diálogo nativo de selección de archivo. */
    import: (sourcePath: string) => invoke<string>("import_optifine", { sourcePath }),
    listImports: (minecraftVersion: string) => invoke<string[]>("list_optifine_imports", { minecraftVersion }),
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
    startMicrosoftLogin: () => invoke<DeviceCodeInfo>("start_microsoft_login"),
    completeMicrosoftLogin: () => invoke<Account>("complete_microsoft_login"),
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
