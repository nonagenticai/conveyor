// Test-only external-provider double for the conveyor copilot E2E path.
//
// The copilot's one model call is `completions.create`, an external action that POSTs to
// `${CONVEYOR_EXTERNAL_INFERENCE_URL}/api/chat/completions`. This is a provider double, not a
// mock of the conveyor API: the conversation, the turn rows, the run and the SSE event are all
// real. Start it before the API:
//
//   node apps/web/e2e/model-stub.mjs &
//   CONVEYOR_EXTERNAL_INFERENCE_URL=http://127.0.0.1:8940 ./conveyor serve
import { createServer } from "node:http";

const PORT = Number(process.env.MODEL_STUB_PORT ?? 8940);

const server = createServer((req, res) => {
  if (req.method === "POST" && req.url === "/api/chat/completions") {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          model: "stub-model",
          choices: [
            { message: { role: "assistant", content: "Stub answer: the copilot path works." } },
          ],
        }),
      );
    });
    return;
  }
  res.writeHead(404, { "content-type": "text/plain" });
  res.end("not found");
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`copilot model stub on ${PORT}`);
});
