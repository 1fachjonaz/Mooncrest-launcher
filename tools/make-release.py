"""Add a new release of a product to the online catalog.

Packs a build folder (for City Life: the "Windows" folder from Unreal's
"Package Project") into a zip, computes SHA-256 and size, and writes the
release entry at the top of `releases` in deploy/mooncrest/v1/products.json.

    python tools/make-release.py city-life 0.1.0 "D:\\Builds\\CityLife\\Windows" --notes "Erster spielbarer Build"
    python tools/make-release.py mlbclient 0.4.0 "D:\\MLBClient\\Launcher\\Beta 3\\setup.exe"

Then upload deploy/mooncrest/ (products.json + files/). Launchers pick the
update up on their next catalog refresh.
"""
import argparse
import datetime
import hashlib
import json
import os
import shutil
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CATALOG = os.path.join(ROOT, "deploy", "mooncrest", "v1", "products.json")
FILES = os.path.join(ROOT, "deploy", "mooncrest", "v1", "files")
DEFAULT_BASE = "https://mlbclient.vercel.app/mooncrest/v1/files/"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def zip_folder(folder, dest):
    root_name = os.path.basename(os.path.normpath(folder))
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for base, _, files in os.walk(folder):
            for name in files:
                full = os.path.join(base, name)
                rel = os.path.relpath(full, folder)
                # One top-level folder in the archive; the catalog uses strip_root.
                z.write(full, os.path.join(root_name, rel))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("product")
    ap.add_argument("version")
    ap.add_argument("source", help="build folder, .zip or setup .exe")
    ap.add_argument("--notes", action="append", default=[], help="release note line (repeatable)")
    ap.add_argument("--title")
    ap.add_argument("--channel", default="beta")
    ap.add_argument("--url-base", default=DEFAULT_BASE)
    ap.add_argument("--platform", default="windows-x86_64")
    a = ap.parse_args()

    with open(CATALOG, encoding="utf-8") as f:
        catalog = json.load(f)
    product = next((p for p in catalog["products"] if p["id"] == a.product), None)
    if not product:
        sys.exit(f"unknown product '{a.product}'. Known: {', '.join(p['id'] for p in catalog['products'])}")
    if any(r["version"] == a.version for r in product["releases"]):
        sys.exit(f"{a.product} {a.version} is already in the catalog")

    os.makedirs(FILES, exist_ok=True)
    if os.path.isdir(a.source):
        if product["install"]["type"] != "zip":
            sys.exit("a folder can only be released for products with install type 'zip'")
        file_name = f"{a.product}-{a.version}-win64.zip"
        dest = os.path.join(FILES, file_name)
        print(f"packing {a.source} -> {dest}")
        zip_folder(a.source, dest)
    else:
        file_name = os.path.basename(a.source).replace(" ", "-")
        dest = os.path.join(FILES, file_name)
        shutil.copyfile(a.source, dest)

    exe = product["install"].get("main_binary")
    if exe and dest.endswith(".zip"):
        with zipfile.ZipFile(dest) as z:
            names = [n.split("/", 1)[-1] if product["install"].get("strip_root") else n for n in z.namelist()]
            if exe not in names:
                sys.exit(f"{exe} is not at the top of the build – check the folder (expected e.g. Windows\\{exe})")

    release = {
        "version": a.version,
        "date": datetime.date.today().isoformat(),
        "channel": a.channel,
        "title": a.title or f"{product['name']} {a.version}",
        "notes": a.notes,
        "files": [{
            "platform": a.platform,
            "file_name": file_name,
            "url": a.url_base.rstrip("/") + "/" + file_name,
            "sha256": sha256(dest),
            "size": os.path.getsize(dest),
        }],
    }
    product["releases"].insert(0, release)
    product.pop("status", None)  # downloadable now
    catalog["generated"] = datetime.datetime.now().strftime("%Y-%m-%dT%H:%M")
    with open(CATALOG, "w", encoding="utf-8") as f:
        json.dump(catalog, f, indent=2, ensure_ascii=False)
    print(json.dumps(release, indent=2))
    print(f"\nupdated {CATALOG}\nnow upload deploy/mooncrest/ to the server")


if __name__ == "__main__":
    main()
