import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CacheInfo, CatalogState, ErrorInfo, InstallRecord, Job, LauncherStatus, LibraryItem, NewsState, Profile, Settings,
} from "./types";

/** Every command error arrives as { code, message }. */
export function asError(e: unknown): ErrorInfo {
  if (e && typeof e === "object" && "code" in e && "message" in e) return e as ErrorInfo;
  return { code: "io", message: String(e) };
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  resetSettings: () => call<Settings>("reset_settings"),
  getCatalog: () => call<CatalogState>("get_catalog"),
  refreshCatalog: () => call<CatalogState>("refresh_catalog"),
  getNews: () => call<NewsState>("get_news"),
  refreshNews: () => call<NewsState>("refresh_news"),
  getLibrary: () => call<LibraryItem[]>("get_library"),
  defaultInstallDir: (productId: string) => call<string>("default_install_dir", { productId }),
  startInstall: (productId: string, installDir?: string | null) => call<Job>("start_install", { productId, installDir: installDir ?? null }),
  installFromFile: (productId: string, file: string, installDir?: string | null) =>
    call<Job>("install_from_file", { productId, file, installDir: installDir ?? null }),
  locateInstall: (productId: string, dir: string) => call<InstallRecord>("locate_install", { productId, dir }),
  uninstall: (productId: string) => call<void>("uninstall_product", { productId }),
  launch: (productId: string) => call<void>("launch_product", { productId }),
  getJobs: () => call<Job[]>("get_jobs"),
  pauseJob: (id: string) => call<void>("pause_job", { id }),
  resumeJob: (id: string) => call<void>("resume_job", { id }),
  cancelJob: (id: string) => call<void>("cancel_job", { id }),
  retryJob: (id: string) => call<void>("retry_job", { id }),
  removeJob: (id: string) => call<void>("remove_job", { id }),
  clearJobHistory: () => call<void>("clear_job_history"),
  launcherStatus: () => call<LauncherStatus>("launcher_status"),
  downloadLauncherUpdate: () => call<Job>("download_launcher_update"),
  applyLauncherUpdate: () => call<void>("apply_launcher_update"),
  cacheInfo: () => call<CacheInfo>("cache_info"),
  clearCache: (kind: "installers" | "catalog" | "logs") => call<CacheInfo>("clear_cache", { kind }),
  getProfile: () => call<Profile>("get_profile"),
  saveProfile: (profile: Profile) => call<Profile>("save_profile", { profile }),
  resetStats: () => call<void>("reset_stats"),
  openPath: (path: string) => call<void>("open_path", { path }),
  openUrl: (url: string) => call<void>("open_url", { url }),
  freeSpace: (path: string) => call<number | null>("free_space", { path }),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}
