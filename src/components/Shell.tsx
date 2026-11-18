import { getCurrentWindow } from "@tauri-apps/api/window";
import { useState } from "react";
import { useApp } from "../store";
import type { Page } from "../types";
import { Crest } from "./Crest";
import { Icon, Mark, type IconName } from "./Icon";
import { jobProgress } from "./ProductAction";

const NAV: { page: Page; icon: IconName }[] = [
  { page: "home", icon: "home" },
  { page: "library", icon: "library" },
  { page: "discover", icon: "discover" },
  { page: "downloads", icon: "download" },
  { page: "news", icon: "news" },
];

export function TitleBar() {
  const { t, canGoBack, back, go, route } = useApp();
  const win = getCurrentWindow();
  const [q, setQ] = useState("");
  return (
    <header className="titlebar" data-tauri-drag-region>
      <button className="icon-btn titlebar-back" onClick={back} disabled={!canGoBack} aria-label={t("nav.back")} title={t("nav.back")}>
        <Icon name="back" />
      </button>
      <form className="titlebar-search" role="search" onSubmit={(e) => { e.preventDefault(); go({ page: "discover", q: q.trim() }); setQ(""); }}>
        <Icon name="search" size={16} />
        <input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("search.placeholder")} aria-label={t("search.placeholder")}
          key={route.page} />
      </form>
      <div className="titlebar-drag" data-tauri-drag-region />
      <div className="window-btns">
        <button onClick={() => win.minimize()} aria-label={t("window.minimize")} title={t("window.minimize")}><Icon name="minimize" size={16} /></button>
        <button onClick={() => win.toggleMaximize()} aria-label={t("window.maximize")} title={t("window.maximize")}><Icon name="maximize" size={14} /></button>
        <button className="is-close" onClick={() => win.close()} aria-label={t("window.close")} title={t("window.close")}><Icon name="close" size={16} /></button>
      </div>
    </header>
  );
}

export function Rail() {
  const { t, route, go, jobs, library, profile } = useApp();
  const active = jobs.find((j) => ["downloading", "verifying", "installing", "queued"].includes(j.state));
  const updates = library.filter((i) => i.update_available && !i.job).length;
  const running = jobs.filter((j) => !["completed", "failed", "cancelled"].includes(j.state)).length;
  const initial = (profile.display_name.trim()[0] ?? "").toUpperCase();
  const item = (page: Page, icon: IconName, badge?: number) => (
    <button key={page} className={`rail-item ${route.page === page ? "is-current" : ""}`}
      aria-current={route.page === page ? "page" : undefined} onClick={() => go({ page })}>
      <Icon name={icon} size={20} />
      <span>{t(`nav.${page}`)}</span>
      {badge ? <em className="rail-badge" aria-label={String(badge)}>{badge}</em> : null}
    </button>
  );
  return (
    <nav className="rail" aria-label="Mooncrest">
      <div className="rail-brand" data-tauri-drag-region>
        <Mark size={30} />
        <div className="rail-brand-text" data-tauri-drag-region>
          <strong>Mooncrest</strong>
          <span>Beyond the Horizon.</span>
        </div>
      </div>
      <div className="rail-nav">
        {NAV.map((n) => item(n.page, n.icon, n.page === "downloads" ? running : n.page === "library" ? updates : undefined))}
      </div>
      <div className="rail-foot">
        {active && (
          <button className="rail-activity" onClick={() => go({ page: "downloads" })}>
            <Crest value={jobProgress(active)} size={44} label={active.product_name} />
            <span>
              <strong>{active.product_name}</strong>
              <small>{t(`state.${active.state}`)}{jobProgress(active) != null && active.state !== "installing" ? ` ${Math.floor((jobProgress(active) ?? 0) * 100)} %` : ""}</small>
            </span>
          </button>
        )}
        {item("settings", "settings")}
        <button className={`rail-item rail-account ${route.page === "account" ? "is-current" : ""}`} onClick={() => go({ page: "account" })}
          aria-current={route.page === "account" ? "page" : undefined}>
          <span className="avatar" aria-hidden="true">{initial || <Icon name="account" size={16} />}</span>
          <span>{profile.display_name || t("nav.account")}</span>
        </button>
      </div>
    </nav>
  );
}

export function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((x) => (
        <div key={x.id} className={`toast ${x.kind === "error" ? "is-error" : ""}`}>
          <Icon name={x.kind === "error" ? "alert" : "check"} size={16} />
          <span>{x.text}</span>
        </div>
      ))}
    </div>
  );
}
