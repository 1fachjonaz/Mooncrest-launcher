import { useState } from "react";
import { api } from "../api";
import { bytes, dateTime } from "../format";
import { useApp } from "../store";
import type { Job, LibraryItem, Product } from "../types";
import { Horizon } from "./Crest";
import { Icon } from "./Icon";
import { InstallDialog } from "./InstallDialog";

export type Status =
  | { kind: "job"; job: Job }
  | { kind: "install" }
  | { kind: "play" }
  | { kind: "update"; to: string }
  | { kind: "running" }
  | { kind: "not_yet"; from: string }
  | { kind: "unavailable" };

export function statusOf(item: LibraryItem | undefined): Status {
  if (!item) return { kind: "unavailable" };
  if (item.job) return { kind: "job", job: item.job };
  if (!item.install) {
    if (!item.available) return { kind: "unavailable" };
    if (item.available_from && Date.parse(item.available_from) > Date.now()) return { kind: "not_yet", from: item.available_from };
    return { kind: "install" };
  }
  if (item.running) return { kind: "running" };
  if (item.update_available && item.latest) return { kind: "update", to: item.latest };
  return { kind: "play" };
}

export function jobProgress(job: Job): number | null {
  if (job.state === "installing" || job.state === "queued") return null;
  if (job.state === "ready" || job.state === "waiting_for_exit" || job.state === "completed") return 1;
  return job.size > 0 ? job.downloaded / job.size : null;
}

/** The main button for a product, with the full install/update/play flow behind it. */
export function ProductAction({ product, large, block }: { product: Product; large?: boolean; block?: boolean }) {
  const { item, t, fail, toast, settings, go } = useApp();
  const it = item(product.id);
  const st = statusOf(it);
  const [dialog, setDialog] = useState(false);
  const [busy, setBusy] = useState(false);
  const cls = `btn btn-primary ${large ? "btn-large" : ""} ${block ? "btn-block" : ""}`;

  const run = async (f: () => Promise<unknown>, ok?: string) => {
    setBusy(true);
    try { await f(); if (ok) toast(ok); } catch (e) { fail(e); } finally { setBusy(false); }
  };

  if (st.kind === "job") {
    const j = st.job;
    const p = jobProgress(j);
    const canPause = j.state === "downloading" || j.state === "queued" || j.state === "verifying";
    return (
      <div className={`action-progress ${large ? "is-large" : ""} ${block ? "is-block" : ""}`}>
        <div className="action-progress-head">
          <span className="action-progress-state">{t(`state.${j.state}`)}</span>
          {j.state === "downloading" && <span className="num">{bytes(j.downloaded, settings.language)} / {bytes(j.size, settings.language)}</span>}
          {p != null && j.state !== "waiting_for_exit" && <span className="num">{Math.floor(p * 100)} %</span>}
        </div>
        <Horizon value={p} />
        <div className="action-progress-btns">
          {canPause && <button className="btn btn-quiet" disabled={busy} onClick={() => run(() => api.pauseJob(j.id))}><Icon name="pause" size={14} />{t("action.pause")}</button>}
          {j.state === "paused" && <button className="btn btn-quiet" disabled={busy} onClick={() => run(() => api.resumeJob(j.id))}><Icon name="play" size={14} />{t("action.resume")}</button>}
          {j.state === "failed" && <button className="btn btn-quiet" disabled={busy} onClick={() => run(() => api.retryJob(j.id))}><Icon name="refresh" size={14} />{t("action.retry")}</button>}
          {j.state !== "installing" && <button className="btn btn-quiet" disabled={busy} onClick={() => run(() => api.cancelJob(j.id))}>{t("action.cancel")}</button>}
          <button className="btn btn-quiet" onClick={() => go({ page: "downloads" })}>{t("nav.downloads")}</button>
        </div>
      </div>
    );
  }

  switch (st.kind) {
    case "install":
      return <>
        <button className={cls} onClick={() => setDialog(true)}><Icon name="download" />{t("action.install")}</button>
        {dialog && <InstallDialog product={product} onClose={() => setDialog(false)} />}
      </>;
    case "play":
      return <button className={cls} disabled={busy} onClick={() => run(() => api.launch(product.id), t("product.launched", { name: product.name }))}><Icon name="play" />{t("action.play")}</button>;
    case "running":
      return <button className={`${cls} is-running`} disabled><span className="pulse" aria-hidden="true" />{t("action.running")}</button>;
    case "update":
      return (
        <div className={`action-pair ${block ? "is-block" : ""}`}>
          <button className={cls} disabled={busy} onClick={() => run(() => api.startInstall(product.id), t("product.queued", { name: product.name }))}><Icon name="update" />{t("action.update")}</button>
          <button className={`btn ${large ? "btn-large" : ""} ${block ? "btn-block" : ""}`} disabled={busy} onClick={() => run(() => api.launch(product.id))}><Icon name="play" />{t("action.play")}</button>
        </div>
      );
    case "not_yet":
      return <span className={`action-note ${large ? "is-large" : ""} ${block ? "is-block" : ""}`}>{t("state.availableFrom", { d: dateTime(st.from, settings.language) })}</span>;
    case "unavailable":
      return <span className={`action-note ${large ? "is-large" : ""} ${block ? "is-block" : ""}`}>
        {product.status === "in_development" ? t("state.inDevelopment") : t("state.unavailable")}
      </span>;
  }
}
