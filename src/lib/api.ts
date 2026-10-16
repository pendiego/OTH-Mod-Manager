import { invoke } from "@tauri-apps/api/core";

export type Loader = "melon" | "bepinex";
export type GameInfo = { dir: string | null; liveriesDir: string };
export type LoaderStatus = { loader: Loader; label: string; installed: boolean; active: boolean; generated: boolean };
export type ModEntry = { id: string; name: string; loader: Loader; enabled: boolean; managed: boolean; nexusId: number | null; version: string | null };
export type InstallResult = { names: string[]; loader: Loader | null };
export type LiveryPack = {
  folder: string;
  enabled: boolean;
  entries: { car: string; name: string }[];
  thumb: string | null;
  error: string | null;
};
export type NexusMod = {
  modId: number;
  name: string;
  summary: string;
  version: string;
  author: string;
  thumbnailUrl: string | null;
  endorsements: number;
  downloads: number;
  updatedAt: string;
  loader: Loader | null;
  requirements: { name: string; url: string; notes: string; modId: number | null }[];
};
export type Account = { name: string; premium: boolean };

export const api = {
  getGame: () => invoke<GameInfo>("get_game"),
  setGameDir: (dir: string) => invoke<GameInfo>("set_game_dir", { dir }),
  loadersStatus: () => invoke<LoaderStatus[]>("loaders_status"),
  installLoader: (loader: Loader) => invoke<void>("install_loader", { loader }),
  setActiveLoader: (loader: Loader | null) => invoke<void>("set_active_loader", { loader }),
  uninstallLoader: (loader: Loader) => invoke<void>("uninstall_loader", { loader }),
  listMods: () => invoke<ModEntry[]>("list_mods"),
  setModEnabled: (id: string, enabled: boolean) => invoke<void>("set_mod_enabled", { id, enabled }),
  uninstallMod: (id: string) => invoke<void>("uninstall_mod", { id }),
  installArchive: (path: string) => invoke<InstallResult>("install_archive", { path }),
  listLiveries: () => invoke<LiveryPack[]>("list_liveries"),
  setLiveryEnabled: (folder: string, enabled: boolean) => invoke<void>("set_livery_enabled", { folder, enabled }),
  removeLivery: (folder: string) => invoke<void>("remove_livery", { folder }),
  nexusMods: () => invoke<NexusMod[]>("nexus_mods"),
  nexusAccount: () => invoke<Account | null>("nexus_account"),
  nexusConnect: (key: string) => invoke<Account>("nexus_connect", { key }),
  nexusDisconnect: () => invoke<void>("nexus_disconnect"),
  nexusInstall: (modId: number, fileId?: number, key?: string, expires?: number) => invoke<InstallResult>("nexus_install", { modId, fileId, key, expires }),
  gameArt: () => invoke<string | null>("game_art"),
};
