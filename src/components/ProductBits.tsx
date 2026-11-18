import { ago, duration } from "../format";
import { loc } from "../i18n";
import { useApp } from "../store";
import type { Product } from "../types";
import { ProductAction, statusOf } from "./ProductAction";

/** One-line status for a product: installed version, update, progress. */
export function StatusLine({ product }: { product: Product }) {
  const { item, t, settings } = useApp();
  const it = item(product.id);
  const st = statusOf(it);
  if (st.kind === "job") return <span className="status is-busy">{t(`state.${st.job.state}`)}</span>;
  if (!it?.install) return <span className="status">{product.status === "in_development" ? t("card.inDevelopment") : t("state.notInstalled")}</span>;
  const v = it.install.version;
  return (
    <span className={`status ${st.kind === "update" ? "is-update" : "is-ok"}`}>
      {v ? t("state.installedVersion", { v }) : t("state.unknownVersion")}
      {st.kind === "update" && <> <b>{t("state.updateTo", { v: st.to })}</b></>}
      {st.kind === "running" && <> <b>{t("action.running")}</b></>}
      {it.stats.last_played && st.kind !== "running" && <span className="muted"> {t("library.lastPlayed", { d: ago(it.stats.last_played, t, settings.language) })}</span>}
    </span>
  );
}

/** Compact product row used on Home and in the Library. */
export function ProductRow({ product, showPlaytime }: { product: Product; showPlaytime?: boolean }) {
  const { go, settings, item, t } = useApp();
  const it = item(product.id);
  return (
    <div className="product-row">
      <button className="product-row-main" onClick={() => go({ page: "library", product: product.id })}>
        <img className="product-icon" src={product.media.icon} alt="" />
        <span className="product-row-text">
          <strong>{product.name}</strong>
          <span className="muted">{loc(product.tagline, settings.language)}</span>
          <StatusLine product={product} />
        </span>
      </button>
      {showPlaytime && it && it.stats.play_secs > 0 && (
        <span className="product-row-meta num">{t("library.playtime", { t: duration(it.stats.play_secs, settings.language) })}</span>
      )}
      <div className="product-row-action"><ProductAction product={product} /></div>
    </div>
  );
}
