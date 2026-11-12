// Same-origin development server for the CSR bundle.
//
//   /pkg/*          apps/web/target/site/pkg   (cargo-leptos output, copied back from m1)
//   /style.css      apps/web/style.css
//   /config.json    apps/web/public/config.json
//   everything else apps/web/index.html        (SPA fallback)
//   /api and /health -> http://127.0.0.1:8930
//   POST /__dev/login sets `conveyor_dev=<login>` and redirects to /
//
// cargo-leptos 0.3.11 builds the wasm/js bundle but does NOT copy index.html, so the source
// HTML is served directly. $CONVEYOR_WEB_ROOT can point at a fully assembled site instead.
//
// The API has no CORS layer by design (CONTRACT §11: the UI ships behind the same origin), so
// this proxy is what makes the browser same-origin locally; it is not a second API surface.
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer, request as httpRequest } from "node:http";
import { extname, join, normalize, resolve } from "node:path";

const PORT = Number(process.env.PORT ?? 8931);
const API = process.env.CONVEYOR_API ?? "http://127.0.0.1:8930";
const HERE = process.cwd();
const ROOT = process.env.CONVEYOR_WEB_ROOT
  ? resolve(process.env.CONVEYOR_WEB_ROOT)
  : null;
const PKG = resolve(HERE, "target/site/pkg");
const INDEX = resolve(HERE, "index.html");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".map": "application/json; charset=utf-8",
  ".woff2": "font/woff2",
};

function proxy(req, res) {
  const target = new URL(req.url, API);
  const headers = { ...req.headers, host: new URL(API).host };
  const upstream = httpRequest(
    { protocol: target.protocol, hostname: target.hostname, port: target.port, path: target.pathname + target.search, method: req.method, headers },
    (up) => {
      res.writeHead(up.statusCode ?? 502, up.headers);
      up.pipe(res); // no buffering: this is also the SSE path
    },
  );
  upstream.on("error", (error) => {
    if (!res.headersSent) res.writeHead(502, { "content-type": "text/plain; charset=utf-8" });
    res.end(`conveyor API unreachable: ${error.message}\n`);
  });
  req.pipe(upstream);
}

function sendFile(res, file) {
  if (!existsSync(file) || !statSync(file).isFile()) return false;
  res.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(res);
  return true;
}

function staticFile(req, res) {
  const rawPath = decodeURIComponent(new URL(req.url, "http://127.0.0.1").pathname);
  const safe = normalize(rawPath).replace(/^(\.\.[/\\])+/, "");

  if (ROOT) {
    const file = join(ROOT, safe);
    if (file.startsWith(ROOT) && sendFile(res, file)) return;
    if (sendFile(res, join(ROOT, "index.html"))) return;
  } else {
    if (rawPath.startsWith("/pkg/")) {
      const file = join(PKG, safe.slice("/pkg/".length));
      if (file.startsWith(PKG) && sendFile(res, file)) return;
    } else if (rawPath === "/style.css") {
      if (sendFile(res, join(HERE, "style.css"))) return;
    } else if (rawPath === "/config.json") {
      if (sendFile(res, join(HERE, "public/config.json"))) return;
    }
    // SPA fallback: every client route resolves to the one index.html.
    if (sendFile(res, INDEX)) return;
  }

  res.writeHead(503, { "content-type": "text/plain; charset=utf-8" });
  res.end(`no built bundle at ${PKG}; run: rbuild -- leptos build --release --frontend-only -p conveyor-web\n`);
}

createServer((req, res) => {
  if (req.url === "/__health") {
    res.writeHead(200, { "content-type": "text/plain" }).end("ok");
    return;
  }
  if (req.url?.startsWith("/__dev/login") && req.method === "POST") {
    const login = new URL(req.url, "http://127.0.0.1").searchParams.get("login") ?? "alice";
    if (!/^[A-Za-z0-9._-]{1,128}$/.test(login)) {
      res.writeHead(400).end("invalid dev login");
      return;
    }
    res.writeHead(303, {
      "set-cookie": `conveyor_dev=${login}; Path=/; SameSite=Lax`,
      location: "/",
    }).end();
    return;
  }
  if (req.url?.startsWith("/api") || req.url?.startsWith("/health")) {
    proxy(req, res);
    return;
  }
  staticFile(req, res);
}).listen(PORT, "127.0.0.1", () => {
  console.log(`conveyor-web dev server: http://127.0.0.1:${PORT}  static=${ROOT}  api=${API}`);
});
