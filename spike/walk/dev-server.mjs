// The W1 spike's same-origin dev server (throwaway; deleted before the PR).
//
//   /walk/            -> spike/walk/index.html
//   /walk/<asset>     -> spike/walk/<asset>
//   /walk/<deep link> -> index.html (the scoped fallback the spike must prove)
//   /api, /health, /config.json -> http://127.0.0.1:8930 (same-origin, no CORS)
//   POST /__dev/login -> `conveyor_dev=<login>` + 303 to /walk/
//
// The production fallback is crates/api/src/web.rs (W2); this server proves the spike's own
// deep link and the static copy without changing the app under test.
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer, request as httpRequest } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const PORT = Number(process.env.PORT ?? 8932);
const API = process.env.CONVEYOR_API ?? "http://127.0.0.1:8930";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

function proxy(req, res) {
  const target = new URL(req.url, API);
  const headers = { ...req.headers, host: new URL(API).host };
  const upstream = httpRequest(
    {
      protocol: target.protocol,
      hostname: target.hostname,
      port: target.port,
      path: target.pathname + target.search,
      method: req.method,
      headers,
    },
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
  const url = new URL(req.url, "http://127.0.0.1");
  const rawPath = decodeURIComponent(url.pathname);
  const safe = normalize(rawPath).replace(/^(\.\.[/\\])+/, "");

  if (rawPath === "/") {
    res.writeHead(302, { location: "/walk/" }).end();
    return;
  }
  if (rawPath === "/walk") {
    res.writeHead(308, { location: `/walk/${url.search}` }).end();
    return;
  }
  if (rawPath.startsWith("/walk/")) {
    const rel = safe.slice("/walk/".length);
    if (rel) {
      const file = join(HERE, rel);
      if (file.startsWith(HERE) && sendFile(res, file)) return;
    }
    if (!extname(rawPath) && sendFile(res, join(HERE, "index.html"))) return;
    res.writeHead(404, { "content-type": "text/plain; charset=utf-8" }).end("not found\n");
    return;
  }
  res.writeHead(404, { "content-type": "text/plain; charset=utf-8" }).end("not found\n");
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
    res
      .writeHead(303, {
        "set-cookie": `conveyor_dev=${login}; Path=/; SameSite=Lax`,
        location: "/walk/",
      })
      .end();
    return;
  }
  if (
    req.url?.startsWith("/api") ||
    req.url?.startsWith("/health") ||
    req.url === "/config.json"
  ) {
    proxy(req, res);
    return;
  }
  staticFile(req, res);
}).listen(PORT, "127.0.0.1", () => {
  console.log(`walk spike: http://127.0.0.1:${PORT}/walk/  api=${API}`);
});
