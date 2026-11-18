"""Local test server for the Mooncrest Launcher (development only).

Serves a folder with HTTP Range support and an optional speed limit, so
downloads, pause/resume and interrupted transfers can be tried for real.

    python serve.py <folder> [--port 8765] [--rate 4000000]
"""
import argparse
import http.server
import os
import re
import time


class Handler(http.server.SimpleHTTPRequestHandler):
    rate = 0  # bytes per second, 0 = unlimited

    def send_head(self):
        path = self.translate_path(self.path)
        if os.path.isdir(path) or not os.path.exists(path):
            return super().send_head()
        size = os.path.getsize(path)
        start, end = 0, size - 1
        rng = self.headers.get("Range")
        m = re.match(r"bytes=(\d+)-(\d*)", rng or "")
        if m:
            start = int(m.group(1))
            if m.group(2):
                end = min(int(m.group(2)), size - 1)
            if start >= size:
                self.send_response(416)
                self.send_header("Content-Range", f"bytes */{size}")
                self.end_headers()
                return None
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        else:
            self.send_response(200)
        ctype = "application/json" if path.endswith(".json") else "application/octet-stream"
        if path.endswith((".jpg", ".svg")):
            ctype = self.guess_type(path)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(end - start + 1))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        f = open(path, "rb")
        f.seek(start)
        self._remaining = end - start + 1
        return f

    def copyfile(self, source, outputfile):
        remaining = getattr(self, "_remaining", None)
        chunk = 64 * 1024
        while remaining is None or remaining > 0:
            n = chunk if remaining is None else min(chunk, remaining)
            data = source.read(n)
            if not data:
                break
            try:
                outputfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                return
            if remaining is not None:
                remaining -= len(data)
            if self.rate:
                time.sleep(len(data) / self.rate)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("folder")
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--rate", type=int, default=0)
    a = ap.parse_args()
    Handler.rate = a.rate
    os.chdir(a.folder)
    http.server.ThreadingHTTPServer(("127.0.0.1", a.port), Handler).serve_forever()
