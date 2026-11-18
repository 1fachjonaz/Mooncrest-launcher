export type Localized<T> = Record<string, T>;
export type Lang = "de" | "en";

export interface ErrorInfo { code: string; message: string }

export interface ReleaseFile { platform: string; file_name: string; url: string; sha256: string; size: number }
export interface Release {
  version: string; date?: string | null; channel?: string | null; title?: string | null;
  notes: string[]; available_from?: string | null; files: ReleaseFile[];
}
export interface Screenshot { src: string; caption: Localized<string> }
export interface Product {
  id: string; name: string; kind: "game" | "software" | "mod"; featured: boolean; status?: "in_development" | null;
  developer?: string | null; publisher?: string | null; website?: string | null;
  support_url?: string | null; community_url?: string | null;
  tagline: Localized<string>; description: Localized<string[]>; highlights: Localized<string[]>;
  includes: string[]; requirements: Localized<string[]>; tags: string[];
  media: { icon: string; cover?: string | null; logo?: string | null; hero: string; accent?: string | null; screenshots: Screenshot[] };
  install: { type: "nsis" | "zip" | "file"; folder: string; main_binary?: string; registry_name?: string };
  releases: Release[];
}
export interface Manifest { schema: number; generated?: string | null; publisher?: string | null; launcher?: { latest: Release } | null; products: Product[] }
export type Source = "online" | "cache" | "bundled";
export interface CatalogState { manifest: Manifest; source: Source; fetched_at: number | null; error: ErrorInfo | null }

export interface NewsEntry {
  id: string; date: string; category?: string | null; product?: string | null; url?: string | null;
  image?: string | null; title: Localized<string>; summary: Localized<string>;
}
export interface NewsState { feed: { schema: number; entries: NewsEntry[] }; source: Source; fetched_at: number | null; error: ErrorInfo | null }

export type JobState = "queued" | "downloading" | "paused" | "verifying" | "ready" | "waiting_for_exit" | "installing" | "completed" | "failed" | "cancelled";
export interface Job {
  id: string; product_id: string; product_name: string; version: string; kind: "install" | "update" | "launcher";
  file_name: string; url: string; sha256: string; size: number; dest: string; local_source: string | null;
  install_dir: string | null; state: JobState; downloaded: number; speed: number; eta_secs: number | null;
  error: ErrorInfo | null; note: string | null; created_at: number; updated_at: number; finished_at: number | null; resumed: boolean;
}

export interface InstallRecord {
  product_id: string; version: string | null; path: string; source: "managed" | "detected" | "manual";
  installed_at: number; updated_at: number;
}
export interface PlayStats { last_played: number | null; play_secs: number; launches: number }
export interface LibraryItem {
  product_id: string; install: InstallRecord | null; latest: string | null; update_available: boolean;
  available: boolean; available_from: string | null; running: boolean; stats: PlayStats; job: Job | null;
}

export type LaunchBehavior = "keep" | "minimize" | "tray";
export interface Settings {
  schema: number; download_dir: string; install_dir: string; auto_update_products: boolean;
  auto_update_launcher: boolean; keep_installers: boolean; launch_on_startup: boolean; start_minimized: boolean;
  close_to_tray: boolean; on_product_launch: LaunchBehavior; language: Lang; theme: "night" | "midnight" | "dawn";
  reduce_motion: boolean; notifications: boolean; notify_downloads: boolean; notify_updates: boolean; catalog_url: string;
}

export interface LauncherStatus {
  version: string; latest: string | null; update_available: boolean; job: Job | null;
  platform: string; data_dir: string; log_file: string | null;
}
export interface CacheInfo {
  installers: number; catalog: number; logs: number; download_dir: string;
  free_download: number | null; free_install: number | null;
}
export interface Profile { display_name: string; created_at: number }

export type Page = "home" | "library" | "discover" | "downloads" | "news" | "settings" | "account";
export interface Route { page: Page; product?: string; q?: string }
