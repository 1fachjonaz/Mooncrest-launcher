import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, asError, on } from "./api";
import { errorText, makeT, type T } from "./i18n";
import type { CatalogState, ErrorInfo, Job, LauncherStatus, LibraryItem, NewsState, Product, Profile, Route, Settings } from "./types";

export interface Toast { id: number; kind: "info" | "error"; text: string }

interface Ctx {
  ready: boolean;
  t: T;
  settings: Settings;
  setSettings: (s: Settings) => Promise<boolean>;
  catalog: CatalogState;
  products: Product[];
  product: (id: string) => Product | undefined;
  library: LibraryItem[];
  item: (id: string) => LibraryItem | undefined;
  jobs: Job[];
  news: NewsState;
  profile: Profile;
  setProfile: (p: Profile) => Promise<void>;
  launcher: LauncherStatus | null;
  route: Route;
  go: (r: Route) => void;
  back: () => void;
  canGoBack: boolean;
  toasts: Toast[];
  toast: (text: string, kind?: Toast["kind"]) => void;
  fail: (e: unknown) => ErrorInfo;
  reloadLibrary: () => Promise<void>;
  reloadLauncher: () => Promise<void>;
}

const C = createContext<Ctx | null>(null);
export const useApp = () => {
  const c = useContext(C);
  if (!c) throw new Error("no app context");
  return c;
};

export function AppProvider({ children }: { children: ReactNode }) {
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [catalog, setCatalog] = useState<CatalogState | null>(null);
  const [library, setLibrary] = useState<LibraryItem[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [news, setNews] = useState<NewsState | null>(null);
  const [profile, setProfileState] = useState<Profile>({ display_name: "", created_at: 0 });
  const [launcher, setLauncher] = useState<LauncherStatus | null>(null);
  const [history, setHistory] = useState<Route[]>([{ page: "home" }]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastId = useRef(0);

  const t = useMemo(() => makeT(settings?.language ?? "de"), [settings?.language]);

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = ++toastId.current;
    setToasts((ts) => [...ts.slice(-3), { id, kind, text }]);
    setTimeout(() => setToasts((ts) => ts.filter((x) => x.id !== id)), kind === "error" ? 7000 : 4000);
  }, []);

  const fail = useCallback((e: unknown) => {
    const err = asError(e);
    toast(`${errorText(t, err)}${err.code === "invalid" || err.code === "io" || err.code === "install_failed" ? ` (${err.message})` : ""}`, "error");
    return err;
  }, [t, toast]);

  const reloadLibrary = useCallback(async () => {
    try { setLibrary(await api.getLibrary()); } catch (e) { console.error(e); }
  }, []);
  const reloadLauncher = useCallback(async () => {
    try { setLauncher(await api.launcherStatus()); } catch (e) { console.error(e); }
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unsubs: Promise<() => void>[] = [];
    (async () => {
      const [s, c, n, j, p] = await Promise.all([api.getSettings(), api.getCatalog(), api.getNews(), api.getJobs(), api.getProfile()]);
      if (cancelled) return;
      setSettingsState(s); setCatalog(c); setNews(n); setJobs(j); setProfileState(p);
      await Promise.all([reloadLibrary(), reloadLauncher()]);
    })().catch((e) => console.error(e));
    let pending: ReturnType<typeof setTimeout> | null = null;
    let lastStates = "";
    unsubs.push(on<Job[]>("jobs", (js) => {
      setJobs(js);
      // A job changed state: library status (installed version, buttons) may have changed too.
      const sig = js.map((x) => `${x.id}:${x.state}`).join(",");
      if (sig !== lastStates) {
        lastStates = sig;
        if (pending) clearTimeout(pending);
        pending = setTimeout(() => { reloadLibrary(); reloadLauncher(); }, 150);
      }
    }));
    unsubs.push(on("library", () => { reloadLibrary(); }));
    unsubs.push(on<CatalogState>("catalog", (c) => { setCatalog(c); reloadLauncher(); }));
    unsubs.push(on<NewsState>("news", setNews));
    unsubs.push(on("launcher-update-ready", () => reloadLauncher()));
    const poll = setInterval(reloadLibrary, 15000); // picks up products started or closed outside the launcher
    return () => { cancelled = true; clearInterval(poll); unsubs.forEach((u) => u.then((f) => f())); };
  }, [reloadLibrary, reloadLauncher]);

  useEffect(() => {
    if (!settings) return;
    document.documentElement.dataset.theme = settings.theme;
    document.documentElement.lang = settings.language;
    document.documentElement.dataset.motion = settings.reduce_motion ? "reduced" : "full";
  }, [settings]);

  const setSettings = useCallback(async (s: Settings) => {
    try {
      const saved = await api.saveSettings(s);
      setSettingsState(saved);
      return true;
    } catch (e) { fail(e); return false; }
  }, [fail]);

  const setProfile = useCallback(async (p: Profile) => {
    try { setProfileState(await api.saveProfile(p)); } catch (e) { fail(e); }
  }, [fail]);

  const go = useCallback((r: Route) => {
    setHistory((h) => {
      const cur = h[h.length - 1];
      if (cur.page === r.page && cur.product === r.product && cur.q === r.q) return h;
      return [...h.slice(-30), r];
    });
    document.querySelector(".content")?.scrollTo({ top: 0 });
  }, []);
  const back = useCallback(() => setHistory((h) => (h.length > 1 ? h.slice(0, -1) : h)), []);

  const value = useMemo<Ctx | null>(() => {
    if (!settings || !catalog || !news) return null;
    const products = catalog.manifest.products;
    return {
      ready: true, t, settings, setSettings, catalog, products,
      product: (id) => products.find((p) => p.id === id),
      library, item: (id) => library.find((i) => i.product_id === id),
      jobs, news, profile, setProfile, launcher,
      route: history[history.length - 1], go, back, canGoBack: history.length > 1,
      toasts, toast, fail, reloadLibrary, reloadLauncher,
    };
  }, [settings, catalog, news, t, setSettings, library, jobs, profile, setProfile, launcher, history, go, back, toasts, toast, fail, reloadLibrary, reloadLauncher]);

  if (!value) return <div className="boot" aria-busy="true" />;
  return <C.Provider value={value}>{children}</C.Provider>;
}
