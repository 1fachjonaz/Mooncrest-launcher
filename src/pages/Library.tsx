import { useState } from "react";
import { CoverCard } from "../components/Covers";
import { Icon } from "../components/Icon";
import { ProductRow } from "../components/ProductBits";
import { Segmented } from "../components/Toggle";
import { useApp } from "../store";

type Filter = "all" | "installed" | "updates";
type Sort = "recent" | "name";

function loadView(): "grid" | "list" {
  try { return localStorage.getItem("library.view") === "list" ? "list" : "grid"; } catch { return "grid"; }
}

export function Library() {
  const { t, products, item } = useApp();
  const [filter, setFilter] = useState<Filter>("all");
  const [sort, setSort] = useState<Sort>("recent");
  const [view, setViewState] = useState<"grid" | "list">(loadView);
  const setView = (v: "grid" | "list") => { setViewState(v); try { localStorage.setItem("library.view", v); } catch { /* storage unavailable */ } };

  const list = products.filter((p) => {
    const it = item(p.id);
    if (filter === "installed") return !!it?.install;
    if (filter === "updates") return !!it?.update_available;
    return true;
  });
  const sorted = [...list].sort((a, b) => {
    if (sort === "name") return a.name.localeCompare(b.name);
    const ia = item(a.id), ib = item(b.id);
    return (Number(!!ib?.install) - Number(!!ia?.install))
      || ((ib?.stats.last_played ?? 0) - (ia?.stats.last_played ?? 0))
      || a.name.localeCompare(b.name);
  });

  return (
    <div className="page">
      <header className="page-head">
        <h1>{t("library.title")}</h1>
      </header>
      <div className="toolbar">
        <Segmented<Filter> label={t("library.title")} value={filter} onChange={setFilter} options={[
          { value: "all", label: t("library.filterAll") },
          { value: "installed", label: t("library.filterInstalled") },
          { value: "updates", label: t("library.filterUpdates") },
        ]} />
        <div className="toolbar-right">
          <label className="select">
            <span>{t("library.sort")}:</span>
            <select value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
              <option value="recent">{t("library.sortRecent")}</option>
              <option value="name">{t("library.sortName")}</option>
            </select>
          </label>
          <div className="view-toggle">
            <button className={view === "grid" ? "is-on" : ""} onClick={() => setView("grid")} aria-label={t("library.grid")} title={t("library.grid")} aria-pressed={view === "grid"}><Icon name="grid" size={16} /></button>
            <button className={view === "list" ? "is-on" : ""} onClick={() => setView("list")} aria-label={t("library.list")} title={t("library.list")} aria-pressed={view === "list"}><Icon name="list" size={16} /></button>
          </div>
        </div>
      </div>
      {sorted.length === 0 ? <p className="empty">{t("library.empty")}</p>
        : view === "grid" ? <div className="cover-grid is-library">{sorted.map((p) => <CoverCard key={p.id} product={p} withAction />)}</div>
        : <div className="rows rows-large">{sorted.map((p) => <ProductRow key={p.id} product={p} showPlaytime />)}</div>}
    </div>
  );
}
