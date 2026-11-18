import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { Confirm } from "../components/Modal";
import { Segmented, Toggle } from "../components/Toggle";
import { ago, bytes } from "../format";
import { useApp } from "../store";
import type { CacheInfo, LaunchBehavior, Settings } from "../types";

export function SettingsPage() {
  const { t, settings, setSettings, catalog, launcher, fail, toast, reloadLauncher, jobs } = useApp();
  const lang = settings.language;
  const [cache, setCache] = useState<CacheInfo | null>(null);
  const [url, setUrl] = useState(settings.catalog_url);
  const [checking, setChecking] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);

  useEffect(() => { api.cacheInfo().then(setCache).catch(fail); }, [fail, jobs.length]);
  useEffect(() => setUrl(settings.catalog_url), [settings.catalog_url]);

  const set = async (patch: Partial<Settings>) => {
    if (await setSettings({ ...settings, ...patch })) toast(t("settings.saved"));
  };
  const pickDir = async (key: "download_dir" | "install_dir") => {
    const d = await open({ directory: true, defaultPath: settings[key] });
    if (typeof d === "string") set({ [key]: d });
  };
  const checkNow = async () => {
    setChecking(true);
    try { await api.refreshCatalog(); await reloadLauncher(); } catch (e) { fail(e); } finally { setChecking(false); }
  };
  const clear = async (kind: "installers" | "catalog" | "logs") => {
    try { setCache(await api.clearCache(kind)); toast(t("settings.cache.cleared")); } catch (e) { fail(e); }
  };
  const catalogStatus = catalog.source === "online" && !catalog.error
    ? t("settings.catalogStatus.online", { d: catalog.fetched_at ? ago(catalog.fetched_at, t, lang) : "" })
    : catalog.source === "cache" ? t("settings.catalogStatus.cache", { d: catalog.manifest.generated ?? "?" })
    : t("settings.catalogStatus.bundled");
  const lj = launcher?.job;

  return (
    <div className="page settings">
      <header className="page-head"><h1>{t("settings.title")}</h1></header>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.storage")}</h2>
        <PathSetting label={t("settings.downloadDir")} hint={t("settings.downloadDirHint")} value={settings.download_dir}
          free={cache?.free_download ?? null} onPick={() => pickDir("download_dir")} />
        <PathSetting label={t("settings.installDir")} hint={t("settings.installDirHint")} value={settings.install_dir}
          free={cache?.free_install ?? null} onPick={() => pickDir("install_dir")} />
        <Toggle label={t("settings.keepInstallers")} checked={settings.keep_installers} onChange={(v) => set({ keep_installers: v })} />
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.updates")}</h2>
        <Toggle label={t("settings.autoUpdateProducts")} hint={t("settings.autoUpdateProductsHint")} checked={settings.auto_update_products} onChange={(v) => set({ auto_update_products: v })} />
        <Toggle label={t("settings.autoUpdateLauncher")} checked={settings.auto_update_launcher} onChange={(v) => set({ auto_update_launcher: v })} />
        <div className="field">
          <label className="field-label" htmlFor="catalog-url">{t("settings.catalog")}</label>
          <div className="inline-form">
            <input id="catalog-url" type="url" value={url} spellCheck={false} onChange={(e) => setUrl(e.target.value)} placeholder="https://" />
            <button className="btn" disabled={url === settings.catalog_url} onClick={() => set({ catalog_url: url.trim() })}>{t("action.save")}</button>
          </div>
          <small className="muted">{t("settings.catalogHint")}</small>
          <small className={`source is-${catalog.error ? "cache" : catalog.source}`}>{catalogStatus}</small>
        </div>
        <button className="btn" disabled={checking} onClick={checkNow}><Icon name="refresh" size={16} />{t("settings.checkNow")}</button>
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.startup")}</h2>
        <Toggle label={t("settings.launchOnStartup")} checked={settings.launch_on_startup} onChange={(v) => set({ launch_on_startup: v })} />
        <Toggle label={t("settings.startMinimized")} checked={settings.start_minimized} onChange={(v) => set({ start_minimized: v })} />
        <Toggle label={t("settings.closeToTray")} checked={settings.close_to_tray} onChange={(v) => set({ close_to_tray: v })} />
        <div className="field">
          <span className="field-label">{t("settings.onLaunch")}</span>
          <Segmented<LaunchBehavior> label={t("settings.onLaunch")} value={settings.on_product_launch} onChange={(v) => set({ on_product_launch: v })} options={[
            { value: "keep", label: t("settings.onLaunch.keep") },
            { value: "minimize", label: t("settings.onLaunch.minimize") },
            { value: "tray", label: t("settings.onLaunch.tray") },
          ]} />
        </div>
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.appearance")}</h2>
        <div className="field">
          <span className="field-label">{t("settings.language")}</span>
          <Segmented label={t("settings.language")} value={settings.language} onChange={(v) => set({ language: v })} options={[
            { value: "de", label: "Deutsch" }, { value: "en", label: "English" },
          ]} />
        </div>
        <div className="field">
          <span className="field-label">{t("settings.theme")}</span>
          <Segmented label={t("settings.theme")} value={settings.theme} onChange={(v) => set({ theme: v })} options={[
            { value: "night", label: t("settings.theme.night") },
            { value: "midnight", label: t("settings.theme.midnight") },
            { value: "dawn", label: t("settings.theme.dawn") },
          ]} />
        </div>
        <Toggle label={t("settings.reduceMotion")} checked={settings.reduce_motion} onChange={(v) => set({ reduce_motion: v })} />
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.notifications")}</h2>
        <Toggle label={t("settings.notificationsOn")} checked={settings.notifications} onChange={(v) => set({ notifications: v })} />
        <Toggle label={t("settings.notifyDownloads")} disabled={!settings.notifications} checked={settings.notify_downloads} onChange={(v) => set({ notify_downloads: v })} />
        <Toggle label={t("settings.notifyUpdates")} disabled={!settings.notifications} checked={settings.notify_updates} onChange={(v) => set({ notify_updates: v })} />
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("settings.about")}</h2>
        <div className="about-row">
          <div>
            <strong>Mooncrest Launcher</strong>
            <span className="num">{t("settings.version", { v: launcher?.version ?? "" })}</span>
            <small className="muted">{launcher?.update_available ? t("settings.launcherUpdate", { v: launcher.latest ?? "" }) : t("settings.launcherLatest")}</small>
          </div>
          {launcher?.update_available && !lj && <button className="btn btn-primary" onClick={() => api.downloadLauncherUpdate().catch(fail)}>{t("settings.launcherDownload")}</button>}
          {lj?.state === "ready" && <button className="btn btn-primary" onClick={() => api.applyLauncherUpdate().catch(fail)}>{t("settings.launcherRestart")}</button>}
          {lj && !["ready", "completed", "cancelled"].includes(lj.state) && <span className="muted">{t(`state.${lj.state}`)}</span>}
        </div>
        <h3 className="h-sub">{t("settings.cache")}</h3>
        {cache && <dl className="cache-list">
          {([["installers", cache.installers], ["catalog", cache.catalog], ["logs", cache.logs]] as const).map(([k, v]) => (
            <div key={k}>
              <dt>{t(`settings.cache.${k}`)}</dt>
              <dd className="num">{bytes(v, lang)}</dd>
              <dd><button className="btn btn-quiet" disabled={v === 0} onClick={() => clear(k)}>{t("action.clear")}</button></dd>
            </div>
          ))}
        </dl>}
        {launcher && (
          <div className="field">
            <span className="field-label">{t("settings.dataDir")}</span>
            <div className="path-field">
              <code title={launcher.data_dir}><bdi>{launcher.data_dir}</bdi></code>
              <button className="btn btn-quiet" onClick={() => api.openPath(launcher.data_dir).catch(fail)}><Icon name="folder" size={16} />{t("action.openFolder")}</button>
            </div>
          </div>
        )}
        <button className="btn btn-danger-quiet" onClick={() => setConfirmReset(true)}>{t("settings.reset")}</button>
      </section>

      {confirmReset && <Confirm title={t("settings.reset")} text={t("settings.resetConfirm")} confirmLabel={t("settings.reset")} cancelLabel={t("action.cancel")} danger
        onClose={() => setConfirmReset(false)} onConfirm={async () => { try { await setSettings(await api.resetSettings()); toast(t("settings.saved")); } catch (e) { fail(e); } }} />}
    </div>
  );
}

function PathSetting({ label, hint, value, free, onPick }: { label: string; hint: string; value: string; free: number | null; onPick: () => void }) {
  const { t, settings, fail } = useApp();
  return (
    <div className="field">
      <span className="field-label">{label}</span>
      <div className="path-field">
        <code title={value}><bdi>{value}</bdi></code>
        <button className="btn btn-quiet" onClick={() => api.openPath(value).catch(fail)} aria-label={t("action.openFolder")} title={t("action.openFolder")}><Icon name="folder" size={16} /></button>
        <button className="btn btn-quiet" onClick={onPick}>{t("action.change")}</button>
      </div>
      <small className="muted">{hint}{free != null ? ` ${t("install.free", { size: bytes(free, settings.language) })}.` : ""}</small>
    </div>
  );
}
