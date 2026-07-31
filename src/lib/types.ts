export interface LauncherConfig {
  launcherName: string;
  logoPath: string;
  iconPath: string;
  theme: "dark" | "light";
  primaryColor: string;
  backgroundImage: string | null;
  welcomeText: string;
  supportUrl: string;
  defaultMinRamMb: number;
  defaultMaxRamMb: number;
  autoUpdateJava: boolean;
  showSnapshots: boolean;
  instancesDir: string | null;
  javaDir: string | null;
}

export interface JavaInstallation {
  path: string;
  version: string;
  majorVersion: number;
  arch: string;
  is64bit: boolean;
  managedByLauncher: boolean;
}

export type VersionType = "release" | "snapshot" | "old_beta" | "old_alpha";

export interface VersionEntry {
  id: string;
  type: VersionType;
  url: string;
  releaseTime: string;
}

export type LoaderKind = "vanilla" | "forge" | "neoforge" | "fabric" | "quilt";

export interface Instance {
  id: string;
  name: string;
  minecraftVersion: string;
  loader: LoaderKind;
  loaderVersion: string | null;
  minRamMb: number | null;
  maxRamMb: number | null;
  extraJvmArgs: string | null;
  createdAt: string;
  lastPlayed: string | null;
  totalPlaytimeSecs: number;
}

export type AccountKind = "offline";

export interface Account {
  id: string;
  kind: AccountKind;
  username: string;
  uuid: string;
  accessToken: string;
  skinUrl: string | null;
}

export interface ModEntry {
  fileName: string;
  enabled: boolean;
}

export interface DownloadProgress {
  label: string;
  downloadedBytes: number;
  totalBytes: number | null;
  completedFiles: number;
  totalFiles: number;
}
