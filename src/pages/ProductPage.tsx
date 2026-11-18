import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api } from "../api";
import { coverOf } from "../components/Covers";
import { Icon } from "../components/Icon";
import { InstallDialog } from "../components/InstallDialog";
import { Confirm, Modal } from "../components/Modal";
import { ProductAction, statusOf } from "../components/ProductAction";
import { StatusLine } from "../components/ProductBits";
import { bytes, date, duration } from "../format";
import { loc } from "../i18n";
import { useApp } from "../store";

interface MediaItem { src: string; caption: string }

export function ProductPage({ id }: { id: string }) {
  const { product, item, t, settings, fail, toast, reloadLibrary } = useApp();
  const p = product(id);
  const it = item(id);
  const lang = settings.language;
  const [cur, setCur] = useState(0);
  const [zoom, setZoom] = useState(false);
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const [fileInstall, setFileInstall] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!p) return <div className="page"><p className="empty">—</p></div>;

  const st = statusOf(it);
  const rec = it?.install ?? null;
  const latest = p.releases.find((r) => r.version === it?.latest) ?? p.releases[0];
  const busyJob = st.kind === "job";
  const media: MediaItem[] = [
    { src: p.media.hero, caption: p.name },
    ...p.media.screenshots.map((s) => ({ src: s.src, caption: loc(s.caption, lang) ?? "" })),
  ];
  const step = (d: number) => setCur((c) => (c + d + media.length) % media.length);
  const platform = latest?.files.some((f) => f.platform.startsWith("windows")) ? "Windows" : latest?.files[0]?.platform ?? "—";

  const fromFile = async () => {
    const f = await open({ multiple: false, directory: false, filters: [{ name: "Setup", extensions: ["exe", "zip", "jar"] }], title: t("action.fromFile") });
    if (typeof f !== "string") return;
    if (rec) {
      setBusy(true);
      try { await api.installFromFile(p.id, f); toast(t("product.queued", { name: p.name })); } catch (e) { fail(e); } finally { setBusy(false); }
    } else setFileInstall(f);
  };
  const locate = async () => {
    const d = await open({ directory: true, title: t("action.locate") });
    if (typeof d !== "string") return;
    try {
      const r = await api.locateInstall(p.id, d);
      toast(t("product.located", { v: r.version ?? "?" }));
      reloadLibrary();
    } catch (e) { fail(e); }
  };
  const uninstall = async () => {
    setBusy(true);
    try { await api.uninstall(p.id); toast(t("product.uninstalled", { name: p.name })); reloadLibrary(); } catch (e) { fail(e); } finally { setBusy(false); }
  };

  return (
    <div className="page product" style={{ ["--accent" as string]: p.media.accent ?? "var(--halo)" }}>
      <header className="product-top">
        <h1>{p.name}</h1>
        <div className="tags">
          <span className="tag">{t(`product.kind.${p.kind}`)}</span>
          {p.tags.map((x) => <span className="tag" key={x}>{x}</span>)}
        </div>
      </header>

      <div className="product-cols">
        <div className="product-main">
          <section className="media" aria-label={t("product.screenshots")}>
            <div className="media-stage">
              <button className="media-img" onClick={() => setZoom(true)} aria-label={media[cur].caption}>
                <img src={media[cur].src} alt={media[cur].caption} />
              </button>
              {media.length > 1 && <>
                <button className="media-nav is-prev" onClick={() => step(-1)} aria-label={t("product.prev")}><Icon name="back" /></button>
                <button className="media-nav is-next" onClick={() => step(1)} aria-label={t("product.next")}><Icon name="forward" /></button>
              </>}
            </div>
            <div className="media-thumbs">
              {media.map((m, n) => (
                <button key={m.src} className={n === cur ? "is-on" : ""} onClick={() => setCur(n)} aria-label={m.caption} aria-current={n === cur}>
                  <img src={m.src} alt="" loading="lazy" />
                </button>
              ))}
            </div>
          </section>

          <section>
            <p className="product-tagline">{loc(p.tagline, lang)}</p>
            <div className="prose">{(loc(p.description, lang) ?? []).map((para, i) => <p key={i}>{para}</p>)}</div>
          </section>

          {(loc(p.highlights, lang) ?? []).length > 0 && (
            <section>
              <h2 className="h-section">{t("product.highlights")}</h2>
              <ul className="highlights">{loc(p.highlights, lang)!.map((h, i) => <li key={i}>{h}</li>)}</ul>
            </section>
          )}

          <section>
            <h2 className="h-section">{t("product.releases")}</h2>
            <ol className="releases">
              {p.releases.map((r) => (
                <li key={r.version}>
                  <div className="release-head">
                    <strong>{r.title ?? r.version}</strong>
                    <span className="num muted">{r.version}</span>
                    {r.channel && <span className="tag">{t(r.channel === "beta" ? "product.channel.beta" : "product.channel.stable")}</span>}
                    {r.version === rec?.version && <span className="tag is-ok"><Icon name="check" size={12} />{t("card.installed")}</span>}
                    {r.date && <time className="muted">{date(r.date, lang)}</time>}
                  </div>
                  {r.notes.length ? <ul>{r.notes.map((n, i) => <li key={i}>{n}</li>)}</ul> : <p className="muted small">{t("product.noNotes")}</p>}
                </li>
              ))}
            </ol>
          </section>

          {(loc(p.requirements, lang) ?? []).length > 0 && (
            <section>
              <h2 className="h-section">{t("product.requirements")}</h2>
              <div className="req-box">
                <span className="req-tab">{platform}</span>
                <ul>{loc(p.requirements, lang)!.map((x) => <li key={x}>{x}</li>)}</ul>
                {p.includes.length > 0 && <>
                  <h4>{t("product.includes")}</h4>
                  <ul>{p.includes.map((x) => <li key={x}>{x}</li>)}</ul>
                </>}
              </div>
            </section>
          )}
        </div>

        <aside className="product-aside">
          <div className="aside-logo">
            {p.media.logo ? <img className="aside-logo-mark" src={p.media.logo} alt="" /> : <img className="aside-cover" src={coverOf(p)} alt="" />}
          </div>
          <span className="tag">{t(`product.kind.${p.kind}`)}</span>
          <strong className="price">{t("price.free")}</strong>
          <ProductAction product={p} large block />
          <StatusLine product={p} />

          <dl className="facts">
            {p.developer && <div><dt>{t("product.developer")}</dt><dd>{p.developer}</dd></div>}
            {p.publisher && <div><dt>{t("product.publisher")}</dt><dd>{p.publisher}</dd></div>}
            {latest?.date && <div><dt>{t("product.releaseDate")}</dt><dd>{date(latest.date, lang)}</dd></div>}
            <div><dt>{t("product.version")}</dt><dd className="num">{it?.latest ?? latest?.version ?? "—"}</dd></div>
            {rec && <div><dt>{t("product.installedVersion")}</dt><dd className="num">{rec.version ?? t("state.unknownVersion")}</dd></div>}
            {latest?.files[0] && <div><dt>{t("product.size")}</dt><dd className="num">{bytes(latest.files[0].size, lang)}</dd></div>}
            <div><dt>{t("product.platform")}</dt><dd>{platform}</dd></div>
            {it && it.stats.launches > 0 && <div><dt>{t("product.playtime")}</dt><dd className="num">{duration(it.stats.play_secs, lang)}</dd></div>}
          </dl>

          {rec && (
            <div className="aside-block">
              <span className="field-label">{t("product.path")}</span>
              <div className="path-field">
                <code title={rec.path}><bdi>{rec.path}</bdi></code>
                <button className="icon-btn" onClick={() => api.openPath(rec.path).catch(fail)} aria-label={t("action.openFolder")} title={t("action.openFolder")}><Icon name="folder" size={16} /></button>
              </div>
              <small className="muted">{t("product.source")}: {t(`product.source.${rec.source}`)}</small>
              <button className="btn btn-danger-quiet" disabled={busy || busyJob || st.kind === "running"} onClick={() => setConfirmUninstall(true)}>
                <Icon name="trash" size={16} />{t("action.uninstall")}
              </button>
            </div>
          )}

          <div className="aside-block">
            <span className="field-label">{t("product.otherWays")}</span>
            <small className="muted">{t("product.fromFileHint")}</small>
            <button className="btn btn-block" disabled={busy || busyJob} onClick={fromFile}><Icon name="file" size={16} />{t("action.fromFile")}</button>
            {!rec && p.install.main_binary && (
              <button className="btn btn-block" disabled={busy || busyJob} onClick={locate} title={t("product.locateHint", { bin: p.install.main_binary })}><Icon name="folder" size={16} />{t("action.locate")}</button>
            )}
          </div>

          <div className="links">
            {p.website && <button className="link" onClick={() => api.openUrl(p.website!)}>{t("action.website")}<Icon name="external" size={14} /></button>}
            {p.community_url && <button className="link" onClick={() => api.openUrl(p.community_url!)}>{t("action.community")}<Icon name="external" size={14} /></button>}
            {p.support_url && <button className="link" onClick={() => api.openUrl(p.support_url!)}>{t("action.support")}<Icon name="external" size={14} /></button>}
          </div>
        </aside>
      </div>

      {zoom && (
        <Modal title={media[cur].caption} onClose={() => setZoom(false)} wide footer={<>
          <span className="muted num">{cur + 1} / {media.length}</span>
          <button className="btn" onClick={() => step(-1)} aria-label={t("product.prev")}><Icon name="back" /></button>
          <button className="btn" onClick={() => step(1)} aria-label={t("product.next")}><Icon name="forward" /></button>
        </>}>
          <img className="lightbox-img" src={media[cur].src} alt={media[cur].caption} />
        </Modal>
      )}
      {confirmUninstall && (
        <Confirm title={t("action.uninstall")} text={t("product.uninstallConfirm", { name: p.name })} danger
          confirmLabel={t("action.uninstall")} cancelLabel={t("action.cancel")} onConfirm={uninstall} onClose={() => setConfirmUninstall(false)} />
      )}
      {fileInstall && <InstallDialog product={p} file={fileInstall} onClose={() => setFileInstall(null)} />}
    </div>
  );
}
