import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../api";
import { bytes } from "../format";
import { useApp } from "../store";
import type { Product } from "../types";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

/** Choose the install folder, see size and free space, then queue the install. */
export function InstallDialog({ product, onClose, file }: { product: Product; onClose: () => void; file?: string }) {
  const { t, settings, fail, toast, go, item } = useApp();
  const [dir, setDir] = useState("");
  const [free, setFree] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const latestVersion = item(product.id)?.latest;
  const latest = product.releases.find((r) => r.version === latestVersion);
  const size = latest?.files[0]?.size ?? 0;

  useEffect(() => { api.defaultInstallDir(product.id).then(setDir).catch(fail); }, [product.id, fail]);
  useEffect(() => {
    if (!dir) return;
    api.freeSpace(dir).then(setFree).catch(() => setFree(null));
  }, [dir]);

  const pick = async () => {
    const chosen = await open({ directory: true, defaultPath: settings.install_dir, title: t("install.pick") });
    if (typeof chosen === "string") {
      // Install into a product folder inside the chosen folder, unless that folder is already named for it.
      const leaf = product.install.folder;
      setDir(chosen.replace(/[\\/]+$/, "").endsWith(leaf) ? chosen : `${chosen.replace(/[\\/]+$/, "")}\\${leaf}`);
    }
  };

  const start = async () => {
    setBusy(true);
    try {
      if (file) await api.installFromFile(product.id, file, dir);
      else await api.startInstall(product.id, dir);
      toast(t("product.queued", { name: product.name }));
      onClose();
      if (!file) go({ page: "downloads" });
    } catch (e) { fail(e); } finally { setBusy(false); }
  };

  const tooSmall = free != null && size > 0 && free < size * 3;
  return (
    <Modal title={t("install.title", { name: product.name })} onClose={onClose} footer={<>
      <button className="btn" onClick={onClose}>{t("action.cancel")}</button>
      <button className="btn btn-primary" disabled={busy || !dir} onClick={start}><Icon name="download" />{t("install.start")}</button>
    </>}>
      <div className="field">
        <span className="field-label">{t("install.folder")}</span>
        <div className="path-field">
          <code title={dir}><bdi>{dir || "…"}</bdi></code>
          <button className="btn btn-quiet" onClick={pick}><Icon name="folder" size={16} />{t("action.change")}</button>
        </div>
        {free != null && <small className={tooSmall ? "text-danger" : "muted"}>{t("install.free", { size: bytes(free, settings.language) })}</small>}
      </div>
      {!file && size > 0 && (
        <dl className="facts">
          <div><dt>{t("install.download")}</dt><dd className="num">{bytes(size, settings.language)}</dd></div>
          <div><dt>{t("product.latestVersion")}</dt><dd>{latest?.version}</dd></div>
        </dl>
      )}
      {file && <p className="muted small">{file}</p>}
    </Modal>
  );
}
