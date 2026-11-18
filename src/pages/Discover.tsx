import { useState } from "react";
import { api } from "../api";
import { CoverCard } from "../components/Covers";
import { Icon } from "../components/Icon";
import { loc } from "../i18n";
import { useApp } from "../store";
import type { Product } from "../types";

const KINDS: Product["kind"][] = ["software", "game", "mod"];

export function Discover() {
  const { t, products, settings, route } = useApp();
  const lang = settings.language;
  const [kinds, setKinds] = useState<Product["kind"][]>([]);
  const [q, setQ] = useState(route.q ?? "");
  const query = q.trim().toLowerCase();
  const list = products.filter((p) => (kinds.length === 0 || kinds.includes(p.kind)) && (!query ||
    [p.name, loc(p.tagline, lang) ?? "", ...p.tags, ...(loc(p.description, lang) ?? [])].join(" ").toLowerCase().includes(query)));
  const toggle = (k: Product["kind"]) => setKinds((ks) => (ks.includes(k) ? ks.filter((x) => x !== k) : [...ks, k]));
  const count = (k: Product["kind"]) => products.filter((p) => p.kind === k).length;
  const kindLabel = { software: t("discover.software"), game: t("discover.games"), mod: t("discover.mods") };
  const community = products.find((p) => p.community_url)?.community_url;

  return (
    <div className="page">
      <header className="page-head">
        <h1>{t("discover.title")}</h1>
        <p className="lead">{t("discover.lead")}</p>
      </header>
      <div className="browse">
        <div>
          <p className="muted small results">{list.length === 1 ? t("discover.result") : t("discover.results", { n: list.length })}</p>
          {list.length === 0 ? (
            <p className="empty">{query ? t("discover.noResults", { q }) : t("discover.emptyKind")}</p>
          ) : (
            <div className="cover-grid is-browse">{list.map((p) => <CoverCard key={p.id} product={p} />)}</div>
          )}
        </div>
        <aside className="filters">
          <div className="filters-head">
            <strong>{t("discover.filters")}</strong>
            {(kinds.length > 0 || q) && <button className="link" onClick={() => { setKinds([]); setQ(""); }}>{t("discover.reset")}</button>}
          </div>
          <label className="search">
            <Icon name="search" size={16} />
            <input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("discover.keywords")} aria-label={t("discover.keywords")} />
          </label>
          <fieldset>
            <legend>{t("discover.category")}</legend>
            {KINDS.map((k) => (
              <label key={k} className="check">
                <input type="checkbox" checked={kinds.includes(k)} onChange={() => toggle(k)} />
                <span>{kindLabel[k]}</span>
                <em className="num">{count(k)}</em>
              </label>
            ))}
          </fieldset>
          {community && (
            <fieldset>
              <legend>{t("discover.links")}</legend>
              <button className="link" onClick={() => api.openUrl(community)}>Discord<Icon name="external" size={14} /></button>
            </fieldset>
          )}
        </aside>
      </div>
    </div>
  );
}
