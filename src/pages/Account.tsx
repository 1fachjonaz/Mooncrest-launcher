import { useState } from "react";
import { api } from "../api";
import { Mark } from "../components/Icon";
import { Confirm } from "../components/Modal";
import { date, duration } from "../format";
import { useApp } from "../store";

export function Account() {
  const { t, profile, setProfile, library, products, settings, fail, toast, reloadLibrary } = useApp();
  const lang = settings.language;
  const [name, setName] = useState(profile.display_name);
  const [confirm, setConfirm] = useState(false);
  const total = library.reduce((s, i) => s + i.stats.play_secs, 0);
  const launches = library.reduce((s, i) => s + i.stats.launches, 0);
  const installed = library.filter((i) => i.install).length;
  const initial = (profile.display_name.trim()[0] ?? "").toUpperCase();

  return (
    <div className="page account">
      <header className="page-head">
        <h1>{t("account.title")}</h1>
        <p className="lead">{t("account.lead")}</p>
      </header>

      <section className="account-card">
        <span className="avatar avatar-large" aria-hidden="true">{initial || <Mark size={36} />}</span>
        <form className="field" onSubmit={async (e) => { e.preventDefault(); await setProfile({ ...profile, display_name: name }); toast(t("settings.saved")); }}>
          <label className="field-label" htmlFor="display-name">{t("account.name")}</label>
          <div className="inline-form">
            <input id="display-name" value={name} maxLength={32} onChange={(e) => setName(e.target.value)} placeholder={t("account.namePlaceholder")} />
            <button className="btn" type="submit" disabled={name.trim() === profile.display_name}>{t("action.save")}</button>
          </div>
          <small className="muted">{t("account.nameHint")}{profile.created_at ? ` ${t("account.since", { d: date(profile.created_at, lang) })}` : ""}</small>
        </form>
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("account.stats")}</h2>
        <dl className="stat-line">
          <div><dt>{t("account.totalPlaytime")}</dt><dd className="num">{total ? duration(total, lang) : "0"}</dd></div>
          <div><dt>{t("account.totalLaunches")}</dt><dd className="num">{launches}</dd></div>
          <div><dt>{t("account.installed")}</dt><dd className="num">{installed}</dd></div>
        </dl>
        <h3 className="h-sub">{t("account.perProduct")}</h3>
        <table className="table">
          <tbody>
            {library.map((i) => {
              const p = products.find((x) => x.id === i.product_id);
              return p ? (
                <tr key={i.product_id}>
                  <th scope="row">{p.name}</th>
                  <td className="num">{duration(i.stats.play_secs, lang)}</td>
                  <td className="num">{i.stats.launches}×</td>
                  <td className="muted">{i.stats.last_played ? date(i.stats.last_played, lang) : t("library.never")}</td>
                </tr>
              ) : null;
            })}
          </tbody>
        </table>
        <button className="btn btn-danger-quiet" disabled={launches === 0} onClick={() => setConfirm(true)}>{t("account.resetStats")}</button>
      </section>

      <section className="settings-group">
        <h2 className="h-section">{t("account.online")}</h2>
        <p className="muted">{t("account.onlineText")}</p>
      </section>

      {confirm && <Confirm title={t("account.resetStats")} text={t("account.resetStatsConfirm")} confirmLabel={t("account.resetStats")} cancelLabel={t("action.cancel")} danger
        onClose={() => setConfirm(false)} onConfirm={async () => { try { await api.resetStats(); await reloadLibrary(); } catch (e) { fail(e); } }} />}
    </div>
  );
}
