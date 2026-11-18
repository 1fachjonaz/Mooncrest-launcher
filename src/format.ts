import type { T } from "./i18n";
import type { Lang } from "./types";

export function bytes(n: number, lang: Lang): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  const digits = i === 0 ? 0 : v >= 100 ? 0 : 1;
  return `${v.toLocaleString(lang === "de" ? "de-DE" : "en-US", { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${units[i]}`;
}

export function duration(secs: number, lang: Lang): string {
  if (secs <= 0) return "–";
  if (secs < 60) return lang === "de" ? `${Math.max(1, Math.round(secs))} s` : `${Math.max(1, Math.round(secs))}s`;
  const m = Math.round(secs / 60);
  if (m < 60) return lang === "de" ? `${m} Min.` : `${m} min`;
  const h = Math.floor(m / 60);
  const r = m % 60;
  return lang === "de" ? `${h} Std.${r ? ` ${r} Min.` : ""}` : `${h} h${r ? ` ${r} min` : ""}`;
}

export function date(iso: string | number, lang: Lang, opts: Intl.DateTimeFormatOptions = { day: "numeric", month: "long", year: "numeric" }): string {
  const d = typeof iso === "number" ? new Date(iso) : new Date(iso.length === 10 ? `${iso}T12:00:00` : iso);
  if (Number.isNaN(d.getTime())) return String(iso);
  return d.toLocaleDateString(lang === "de" ? "de-DE" : "en-GB", opts);
}

export function dateTime(iso: string, lang: Lang): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(lang === "de" ? "de-DE" : "en-GB", { day: "numeric", month: "long", hour: "2-digit", minute: "2-digit" });
}

export function ago(ms: number, t: T, lang: Lang): string {
  const diff = (Date.now() - ms) / 1000;
  if (diff < 60) return t("time.justNow");
  if (diff < 3600) return t("time.minutesAgo", { n: Math.round(diff / 60) });
  if (diff < 86400) return t("time.hoursAgo", { n: Math.round(diff / 3600) });
  if (diff < 172800) return t("time.yesterday");
  if (diff < 7 * 86400) return t("time.daysAgo", { n: Math.round(diff / 86400) });
  return date(ms, lang);
}
