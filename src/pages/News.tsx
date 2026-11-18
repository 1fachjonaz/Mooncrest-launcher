import { useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { ago, date } from "../format";
import { loc, type Key } from "../i18n";
import { useApp } from "../store";

export function News() {
  const { t, news, settings, products, fail } = useApp();
  const lang = settings.language;
  const [busy, setBusy] = useState(false);
  const refresh = async () => { setBusy(true); try { await api.refreshNews(); } catch (e) { fail(e); } finally { setBusy(false); } };
  const sourceText = news.source === "online" ? t("news.source.online")
    : news.source === "cache" ? t("news.source.cache", { d: news.fetched_at ? ago(news.fetched_at, t, lang) : "" })
    : t("news.source.bundled");
  const [lead, ...rest] = news.feed.entries;
  const cat = (c?: string | null) => (c ? t(`news.cat.${c}` as Key) : null);
  return (
    <div className="page news-page">
      <header className="page-head">
        <h1>{t("news.title")}</h1>
        <div className="page-head-tools">
          <span className={`source is-${news.source}`}>{sourceText}</span>
          <button className="btn btn-quiet" disabled={busy} onClick={refresh}><Icon name="refresh" size={14} />{t("action.refresh")}</button>
        </div>
      </header>
      {!lead ? <p className="empty">{t("news.empty")}</p> : <>
        <article className="news-lead">
          {lead.image && <img src={lead.image} alt="" />}
          <div className="news-lead-body">
            <span className="tag">{t("news.featured")}</span>
            <time dateTime={lead.date}>{date(lead.date, lang)}</time>
            <h2>{loc(lead.title, lang)}</h2>
            <p>{loc(lead.summary, lang)}</p>
            {lead.url && <button className="btn btn-primary" onClick={() => api.openUrl(lead.url!)}>{t("action.read")}<Icon name="external" size={14} /></button>}
          </div>
        </article>
        <div className="news-cards">
          {rest.map((n) => {
            const p = products.find((x) => x.id === n.product);
            return (
              <article key={n.id} className="news-card">
                <button className="news-card-art" onClick={() => n.url && api.openUrl(n.url)} disabled={!n.url} aria-label={loc(n.title, lang)}>
                  {n.image ? <img src={n.image} alt="" loading="lazy" /> : <span className="news-card-blank" />}
                </button>
                <div className="news-meta">
                  <time dateTime={n.date}>{date(n.date, lang)}</time>
                  {p && <span>{p.name}</span>}
                  {cat(n.category) && <span className="tag">{cat(n.category)}</span>}
                </div>
                <h3>{loc(n.title, lang)}</h3>
                <p>{loc(n.summary, lang)}</p>
                {n.url && <button className="link" onClick={() => api.openUrl(n.url!)}>{t("action.read")}<Icon name="external" size={14} /></button>}
              </article>
            );
          })}
        </div>
      </>}
    </div>
  );
}
