// SSE over `fetch` + `ReadableStream`: `EventSource` cannot carry the bearer (SPEC §5).

/**
 * @typedef {{id: string, event: string, data: string}} SseFrame
 */

/**
 * Parse complete SSE frames out of a buffer. Returns the frames and the unconsumed tail; a
 * frame is separated by a blank line, `data:` lines are joined with a newline, and comments
 * (`:`) are ignored.
 * @param {string} buffer
 * @returns {{frames: SseFrame[], rest: string}}
 */
export function parseSseBuffer(buffer) {
  const frames = [];
  let rest = buffer;
  let boundary = rest.indexOf("\n\n");
  while (boundary >= 0) {
    const block = rest.slice(0, boundary);
    rest = rest.slice(boundary + 2);
    let id = "";
    let event = "message";
    const data = [];
    for (const rawLine of block.split("\n")) {
      const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
      if (line === "" || line.startsWith(":")) continue;
      const colon = line.indexOf(":");
      const field = colon < 0 ? line : line.slice(0, colon);
      let value = colon < 0 ? "" : line.slice(colon + 1);
      if (value.startsWith(" ")) value = value.slice(1);
      if (field === "id") id = value;
      else if (field === "event") event = value;
      else if (field === "data") data.push(value);
    }
    if (data.length > 0) frames.push({ id, event, data: data.join("\n") });
    boundary = rest.indexOf("\n\n");
  }
  return { frames, rest };
}

/**
 * Stream frames until the signal aborts. The caller owns reconnection and resync.
 * @param {string} url
 * @param {{headers?: Record<string,string>, signal?: AbortSignal,
 *          onFrame: (frame: SseFrame) => void}} options
 */
export async function streamSse(url, options) {
  const response = await fetch(url, {
    headers: { accept: "text/event-stream", ...(options.headers ?? {}) },
    credentials: "same-origin",
    cache: "no-store",
    signal: options.signal,
  });
  if (!response.ok || !response.body) {
    throw new Error(`events answered ${response.status}`);
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const parsed = parseSseBuffer(buffer);
    buffer = parsed.rest;
    for (const frame of parsed.frames) options.onFrame(frame);
  }
}
