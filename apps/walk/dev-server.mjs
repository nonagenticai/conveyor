// The walk's local origin (port 8932). It serves the tree at the root — the game host's
// production shape — and proxies the API same-origin to the real server on 8930.
//
// The proxy forces `Host: game.nonagentic.ai` so `/config.json` answers the game client id
// when the server has CONVEYOR_GAME_HOSTS configured. Without that forced host the server
// would classify the request as the operator origin and serve the wrong client id.
//
// Node stdlib only: no dependency, no build step.

import { createServer, request as httpRequest } from "node:http";
import { createReadStream, existsSync, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL(".", import.meta.url)));
const PORT = Number(process.env.PORT ?? 8932);
const API = process.env.WALK_API ?? "http://127.0.0.1:8930";
const PROXY_PREFIXES = ["/api", "/config.json", "/__dev", "/health", "/metrics"];
const PROXY_HOST = process.env.WALK_PROXY_HOST ?? "game.nonagentic.ai";

const TYPES = {
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
  ".map": "application/json; charset=utf-8",
};

function shouldProxy(pathname) {
  return PROXY_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  );
}

function proxy(req, res) {
  // Only the PATH crosses to the upstream. `new URL(req.url, API)` would let an absolute-form
  // or protocol-relative request URL replace the host (`//evil.example/x`), turning this local
  // server into an open proxy (CodeQL `js/request-forgery`). Parse against a fixed origin and
  // rebuild the upstream from pathname + search only.
  const incoming = new URL(req.url, "http://127.0.0.1");
  const target = new URL(incoming.pathname + incoming.search, API);
  const headers = { ...req.headers, host: PROXY_HOST };
  delete headers["accept-encoding"];
  const upstream = httpRequest(
    target,
    { method: req.method, headers },
    (answer) => {
      res.writeHead(answer.statusCode ?? 502, answer.headers);
      answer.pipe(res);
    },
  );
  upstream.on("error", (error) => {
    res.writeHead(502, { "content-type": "text/plain; charset=utf-8" });
    res.end(`proxy error: ${error.message}`);
  });
  req.pipe(upstream);
}

function staticFile(pathname, res) {
  const relative = normalize(pathname).replace(/^(\.\.[/\\])+/, "").replace(/^[/\\]+/, "");
  const file = join(ROOT, relative);
  if (!file.startsWith(ROOT)) {
    res.writeHead(403);
    res.end();
    return;
  }
  if (existsSync(file) && statSync(file).isFile()) {
    const type = TYPES[extname(file).toLowerCase()] ?? "application/octet-stream";
    res.writeHead(200, { "content-type": type, "cache-control": "no-cache" });
    createReadStream(file).pipe(res);
    return;
  }
  if (extname(pathname) !== "") {
    res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    res.end("not found");
    return;
  }
  // Clean URLs: a non-asset path is the walk entry.
  res.writeHead(200, { "content-type": TYPES[".html"], "cache-control": "no-cache" });
  createReadStream(join(ROOT, "index.html")).pipe(res);
}

createServer((req, res) => {
  const pathname = new URL(req.url, `http://127.0.0.1:${PORT}`).pathname;
  if (pathname === "/__health") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end("ok");
    return;
  }
  if (shouldProxy(pathname)) {
    proxy(req, res);
    return;
  }
  staticFile(pathname, res);
}).listen(PORT, "127.0.0.1", () => {
  console.log(`walk dev server: http://127.0.0.1:${PORT} -> ${API} (Host: ${PROXY_HOST})`);
});
