import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { Crest } from "../components/Crest";
import { Icon } from "../components/Icon";
import { jobProgress } from "../components/ProductAction";
import { ago, bytes, duration } from "../format";
import { errorText } from "../i18n";
import { useApp } from "../store";
import type { Job } from "../types";

const ACTIVE = ["downloading", "verifying", "installing", "ready", "waiting_for_exit"];

export function Downloads() {
  const { t, jobs, fail } = useApp();
  const active = jobs.filter((j) => ACTIVE.includes(j.state));
  const queue = jobs.filter((j) => j.state === "queued" || j.state === "paused");
  const history = jobs.filter((j) => ["completed", "failed", "cancelled"].includes(j.state)).sort((a, b) => (b.finished_at ?? 0) - (a.finished_at ?? 0));
  return (
    <div className="page">
      <header className="page-head">
        <h1>{t("downloads.title")}</h1>
        <p className="lead">{t("downloads.lead")}</p>
      </header>
      {active.length === 0 && queue.length === 0 && <p className="empty">{t("downloads.empty")}</p>}
      {active.length > 0 && (
        <section className="dl-section">
          <h2 className="h-section">{t("downloads.active")}</h2>
          {active.map((j) => <JobRow key={j.id} job={j} big />)}
        </section>
      )}
      {queue.length > 0 && (
        <section className="dl-section">
          <h2 className="h-section">{t("downloads.queue")}</h2>
          {queue.map((j) => <JobRow key={j.id} job={j} />)}
        </section>
      )}
      <section className="dl-section">
        <header className="section-head">
          <h2 className="h-section">{t("downloads.history")}</h2>
          {history.length > 0 && <button className="link" onClick={() => api.clearJobHistory().catch(fail)}>{t("downloads.clearHistory")}</button>}
        </header>
        {history.length === 0 ? <p className="empty small">{t("downloads.historyEmpty")}</p> : history.map((j) => <JobRow key={j.id} job={j} />)}
      </section>
    </div>
  );
}

function JobRow({ job: j, big }: { job: Job; big?: boolean }) {
  const { t, settings, fail, go, launcher, product } = useApp();
  const prod = product(j.product_id);
  const lang = settings.language;
  const p = jobProgress(j);
  const act = (f: () => Promise<unknown>) => f().catch(fail);
  const terminal = ["completed", "failed", "cancelled"].includes(j.state);
  return (
    <div className={`job ${big ? "is-big" : ""} is-${j.state}`}>
      {prod ? <img className="job-thumb" src={prod.media.cover ?? prod.media.icon} alt="" /> : <span className="job-thumb is-blank"><Crest value={p} size={40} /></span>}
      {big && <Crest value={p} size={64} label={j.product_name} />}
      <div className="job-main">
        <div className="job-title">
          <button className="link-plain" onClick={() => j.kind !== "launcher" && go({ page: "library", product: j.product_id })}>
            <strong>{j.product_name}</strong>
          </button>
          <span className="num">{j.version}</span>
          <span className="tag">{t(`kind.${j.kind}`)}</span>
          {j.local_source && <span className="tag">{t("downloads.fromFile")}</span>}
        </div>
        <div className="job-status">
          <span className={`state is-${j.state}`}>{t(`state.${j.state}`)}</span>
          {j.state === "downloading" && <>
            <span className="num">{t("downloads.of", { a: bytes(j.downloaded, lang), b: bytes(j.size, lang) })}</span>
            {j.speed > 0 && <span className="num">{t("downloads.speed", { s: bytes(j.speed, lang) })}</span>}
            {j.eta_secs != null && <span className="num">{t("downloads.eta", { t: duration(j.eta_secs, lang) })}</span>}
          </>}
          {(j.state === "paused" || j.state === "queued") && j.downloaded > 0 && <span className="num">{t("downloads.of", { a: bytes(j.downloaded, lang), b: bytes(j.size, lang) })}</span>}
          {j.resumed && !terminal && <span className="muted">{t("downloads.resumed")}</span>}
          {j.note && <span className="muted">{j.note}</span>}
          {j.state === "completed" && <span className="muted"><Icon name="check" size={12} /> {t("downloads.verified")}</span>}
          {terminal && j.finished_at && <span className="muted">{ago(j.finished_at, t, lang)}</span>}
        </div>
        {j.error && j.state === "failed" && (
          <p className="job-error"><Icon name="alert" size={14} /> {errorText(t, j.error)} <span className="muted">{j.error.message}</span></p>
        )}
        {j.error && j.state === "failed" && (j.error.code === "not_found" || j.error.code === "not_available") && j.kind !== "launcher" && (
          <div className="job-hint">
            <span>{t("downloads.notHosted")}</span>
            <button className="btn" onClick={async () => {
              const f = await open({ multiple: false, directory: false, filters: [{ name: "Setup", extensions: ["exe", "zip", "jar"] }], title: t("downloads.pickFile") });
              if (typeof f !== "string") return;
              try {
                await api.removeJob(j.id);
                await api.installFromFile(j.product_id, f, j.install_dir);
              } catch (e) { fail(e); }
            }}><Icon name="file" size={16} />{t("downloads.pickFile")}</button>
          </div>
        )}
        {big && j.state !== "ready" && j.state !== "waiting_for_exit" && <div className="job-bar"><div style={{ width: `${(p ?? 0) * 100}%` }} className={p == null ? "is-indeterminate" : ""} /></div>}
      </div>
      <div className="job-actions">
        {(j.state === "downloading" || j.state === "queued" || j.state === "verifying") && <button className="btn btn-quiet" onClick={() => act(() => api.pauseJob(j.id))}><Icon name="pause" size={14} />{t("action.pause")}</button>}
        {j.state === "paused" && <button className="btn btn-quiet" onClick={() => act(() => api.resumeJob(j.id))}><Icon name="play" size={14} />{t("action.resume")}</button>}
        {j.state === "failed" && <button className="btn btn-quiet" onClick={() => act(() => api.retryJob(j.id))}><Icon name="refresh" size={14} />{t("action.retry")}</button>}
        {j.kind === "launcher" && j.state === "ready" && launcher && <button className="btn btn-primary" onClick={() => act(() => api.applyLauncherUpdate())}>{t("settings.launcherRestart")}</button>}
        {!terminal && j.state !== "installing" && <button className="btn btn-quiet" onClick={() => act(() => api.cancelJob(j.id))}>{t("action.cancel")}</button>}
        {terminal && <button className="icon-btn" aria-label={t("action.remove")} title={t("action.remove")} onClick={() => act(() => api.removeJob(j.id))}><Icon name="close" size={16} /></button>}
      </div>
    </div>
  );
}
