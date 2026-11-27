// SSE over fetch + ReadableStream: the bearer cannot ride EventSource. The parser keeps the
// last incomplete block and emits whole frames only.
import { DEV_TOKEN } from "./api.js";

export async function subscribe(topics, onFrame, opts = {}) {
  const response = await fetch(`/api/events?topics=${encodeURIComponent(topics.join(","))}`, {
    headers: { authorization: `Bearer ${opts.token ?? DEV_TOKEN}` },
    credentials: "same-origin",
    signal: opts.signal,
  });
  if (!response.ok) {
    throw new Error(`SSE ${response.status}: ${await response.text()}`);
  }
  if (!response.body) throw new Error("SSE response has no body");

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const frames = [];
  let buffer = "";
  let closed = false;

  function pump() {
    return reader
      .read()
      .then(({ value, done }) => {
        if (done) {
          closed = true;
          return;
        }
        buffer += decoder.decode(value, { stream: true });
        let index;
        while ((index = buffer.indexOf("\n\n")) >= 0) {
          const block = buffer.slice(0, index);
          buffer = buffer.slice(index + 2);
          const frame = parse(block);
          if (frame) {
            frames.push(frame);
            onFrame?.(frame);
          }
        }
        return pump();
      })
      .catch((error) => {
        if (!opts.signal?.aborted) throw error;
      });
  }
  void pump();

  return {
    frames,
    closed: () => closed,
    close: () => reader.cancel().catch(() => {}),
  };
}

function parse(block) {
  const fields = {};
  for (const line of block.split("\n")) {
    if (!line || line.startsWith(":")) continue;
    const index = line.indexOf(":");
    if (index < 0) continue;
    const key = line.slice(0, index);
    const value = line.slice(index + 1).replace(/^ /, "");
    fields[key] = value;
  }
  if (!fields.data) return null;
  let data;
  try {
    data = JSON.parse(fields.data);
  } catch {
    data = fields.data;
  }
  return { id: fields.id, event: fields.event, data };
}
