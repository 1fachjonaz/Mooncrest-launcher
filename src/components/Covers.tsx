import { useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { useApp } from "../store";
import type { Product } from "../types";
import { Icon } from "./Icon";
import { jobProgress, ProductAction, statusOf } from "./ProductAction";

export function coverOf(p: Product) {
  return p.media.cover ?? p.media.icon;
}

/** Short status under a cover, Epic-style: price for the store, state for the library. */
function CardStatus({ product }: { product: Product }) {
  const { item, t } = useApp();
  const st = statusOf(item(product.id));
  switch (st.kind) {
    case "job": return <span className="card-status is-busy">{t(`state.${st.job.state}`)}</span>;
    case "play": return <span className="card-status is-ok">{t("card.installed")}</span>;
    case "update": return <span className="card-status is-update">{t("card.update")}</span>;
    case "running": return <span className="card-status is-ok">{t("card.running")}</span>;
    default: return <span className="card-status">{product.status === "in_development" ? t("card.inDevelopment") : t("price.free")}</span>;
  }
}

/** Portrait cover card used in shelves, the store grid and the library. */
export function CoverCard({ product, withAction }: { product: Product; withAction?: boolean }) {
  const { go, t, item, fail } = useApp();
  const it = item(product.id);
  const st = statusOf(it);
  const p = st.kind === "job" ? jobProgress(st.job) : null;
  const [menu, setMenu] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const close = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setMenu(false); };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [menu]);
  return (
    <div className="cover-card" ref={ref}>
      <button className="cover-art" onClick={() => go({ page: "library", product: product.id })} aria-label={product.name}>
        <img src={coverOf(product)} alt="" loading="lazy" />
        {st.kind === "job" && <span className="cover-progress"><span style={{ width: `${(p ?? 0) * 100}%` }} /></span>}
      </button>
      <div className="cover-meta">
        <span className="cover-kind">{t(`product.kind.${product.kind}`)}</span>
        <div className="cover-title-row">
          <button className="cover-title" onClick={() => go({ page: "library", product: product.id })}>{product.name}</button>
          {withAction && it?.install && (
            <div className="cover-menu">
              <button className="icon-btn" aria-label={t("card.more")} title={t("card.more")} aria-expanded={menu} onClick={() => setMenu((m) => !m)}>
                <Icon name="more" size={18} />
              </button>
              {menu && (
                <div className="menu" role="menu">
                  <button role="menuitem" onClick={() => { setMenu(false); go({ page: "library", product: product.id }); }}>{t("action.details")}</button>
                  <button role="menuitem" onClick={() => { setMenu(false); api.openPath(it.install!.path).catch(fail); }}>{t("action.openFolder")}</button>
                </div>
              )}
            </div>
          )}
        </div>
        {withAction ? <div className="cover-action"><ProductAction product={product} block /></div> : <CardStatus product={product} />}
      </div>
    </div>
  );
}

export function Shelf({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="shelf">
      <header className="section-head">
        <h2>{title}</h2>
        {action}
      </header>
      <div className="cover-grid">{children}</div>
    </section>
  );
}
