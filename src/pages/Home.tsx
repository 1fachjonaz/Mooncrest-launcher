import { api } from "../api";
import { Carousel, type Slide } from "../components/Carousel";
import { CoverCard, coverOf, Shelf } from "../components/Covers";
import { Icon } from "../components/Icon";
import { ProductAction } from "../components/ProductAction";
import { date } from "../format";
import { loc, type T } from "../i18n";
import { useApp } from "../store";

function greeting(t: T) {
  const h = new Date().getHours();
  if (h < 5) return t("greet.night");
  if (h < 11) return t("greet.morning");
  if (h < 18) return t("greet.day");
  if (h < 23) return t("greet.evening");
  return t("greet.night");
}

export function Home() {
  const { t, settings, profile, products, library, news, go } = useApp();
  const lang = settings.language;
  const name = profile.display_name.trim();
  const installed = library.filter((i) => i.install);
  const byRecent = [...installed].sort((a, b) => (b.stats.last_played ?? 0) - (a.stats.last_played ?? 0));
  const updates = library.filter((i) => i.update_available && !i.job);

  // Featured products first, then news that has artwork.
  const slides: Slide[] = [
    ...[...products.filter((p) => p.featured), ...products.filter((p) => !p.featured)].map<Slide>((p) => ({
      id: `p-${p.id}`, image: p.media.hero, thumb: coverOf(p), logo: p.media.logo,
      eyebrow: t(`product.kind.${p.kind}`), title: p.name, text: loc(p.tagline, lang),
      actions: <>
        <ProductAction product={p} large />
        <button className="btn btn-glass btn-large" onClick={() => go({ page: "library", product: p.id })}>{t("action.details")}</button>
      </>,
    })),
    ...news.feed.entries.filter((n) => n.image).slice(0, Math.max(1, 5 - products.length)).map<Slide>((n) => ({
      id: `n-${n.id}`, image: n.image!, thumb: n.image!,
      eyebrow: date(n.date, lang), title: loc(n.title, lang) ?? "", text: loc(n.summary, lang),
      actions: n.url ? <button className="btn btn-primary btn-large" onClick={() => api.openUrl(n.url!)}>{t("action.read")}<Icon name="external" size={16} /></button> : null,
    })),
  ];

  return (
    <div className="page home">
      <header className="home-head">
        <h1>{greeting(t)}{name ? `, ${name}` : ""}</h1>
        <span className="muted">{date(Date.now(), lang, { weekday: "long", day: "numeric", month: "long" })}</span>
        {updates.length > 0 && (
          <button className="pill" onClick={() => go({ page: "library" })}>
            <Icon name="update" size={14} />
            {t(updates.length === 1 ? "home.updatesWaiting" : "home.updatesWaitingPlural", { n: updates.length })}
          </button>
        )}
      </header>

      <Carousel slides={slides} />

      {byRecent.length > 0 && (
        <Shelf title={t("home.yourLibrary")} action={<button className="link" onClick={() => go({ page: "library" })}>{t("home.seeAll")}</button>}>
          {byRecent.map((i) => {
            const p = products.find((x) => x.id === i.product_id);
            return p ? <CoverCard key={p.id} product={p} withAction /> : null;
          })}
        </Shelf>
      )}

      <Shelf title={t("home.catalog")} action={<button className="link" onClick={() => go({ page: "discover" })}>{t("home.seeAll")}</button>}>
        {products.map((p) => <CoverCard key={p.id} product={p} />)}
      </Shelf>

      <section className="shelf">
        <header className="section-head">
          <h2>{t("home.news")}</h2>
          <button className="link" onClick={() => go({ page: "news" })}>{t("home.allNews")}</button>
        </header>
        <div className="news-cards">
          {news.feed.entries.slice(0, 3).map((n) => (
            <article key={n.id} className="news-card">
              <button className="news-card-art" onClick={() => n.url && api.openUrl(n.url)} disabled={!n.url} aria-label={loc(n.title, lang)}>
                {n.image ? <img src={n.image} alt="" loading="lazy" /> : <span className="news-card-blank" />}
              </button>
              <time dateTime={n.date}>{date(n.date, lang)}</time>
              <h3>{loc(n.title, lang)}</h3>
              <p>{loc(n.summary, lang)}</p>
              {n.url && <button className="link" onClick={() => api.openUrl(n.url!)}>{t("action.read")}<Icon name="external" size={14} /></button>}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}
