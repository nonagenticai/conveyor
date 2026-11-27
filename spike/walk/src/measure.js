// The measurement harness from SPEC §12 / ADR 0023: frames, long tasks, heap, transfer bytes
// and the cold-load mark. The test drives five runs per renderer and a soak.
import * as canvas2d from "./canvas2d.js";
import * as webgl2 from "./webgl2.js";

function renderer(name) {
  return name === "webgl2" ? webgl2 : canvas2d;
}

function quantile(sorted, p) {
  if (sorted.length === 0) return null;
  const index = Math.min(sorted.length - 1, Math.floor(p * sorted.length));
  return sorted[index];
}

export async function measure(name, canvas, seconds = 5) {
  const draw = renderer(name).create(canvas);
  if (!draw) {
    return { available: false, renderer: name, reason: "the context is unavailable" };
  }

  const frames = [];
  const longTasks = [];
  let start = performance.now();
  let last = start;
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) longTasks.push(entry.duration);
  });
  try {
    observer.observe({ type: "longtask", buffered: true });
  } catch {
    // longtask is not available in every engine; the other metrics still stand.
  }

  const heapBefore = performance.memory?.usedJSHeapSize ?? null;
  await new Promise((resolve) => {
    function frame(now) {
      frames.push(now - last);
      last = now;
      draw.draw(now, 0);
      if (now - start < seconds * 1000) requestAnimationFrame(frame);
      else resolve();
    }
    requestAnimationFrame(frame);
  });
  observer.disconnect();

  const body = frames.slice(2).sort((a, b) => a - b);
  const resources = performance
    .getEntriesByType("resource")
    .filter((entry) => entry.name.includes("/walk/"));
  const navigation = performance.getEntriesByType("navigation")[0];
  const firstFrame = performance.getEntriesByName("spike-first-frame")[0]?.startTime ?? null;
  return {
    available: true,
    renderer: name,
    frames: frames.length,
    p50: quantile(body, 0.5),
    p95: quantile(body, 0.95),
    longTasks: { count: longTasks.length, total: longTasks.reduce((a, b) => a + b, 0) },
    heapBefore,
    heapAfter: performance.memory?.usedJSHeapSize ?? null,
    bytes: resources.reduce((total, entry) => total + (entry.encodedBodySize ?? 0), 0),
    requests: resources.length,
    firstFrame,
    coldLoad: navigation && firstFrame !== null ? firstFrame - navigation.startTime : null,
  };
}

export async function soak(name, canvas, roomChanges = 20, holdMs = 300_000) {
  const draw = renderer(name).create(canvas);
  if (!draw) return { available: false, renderer: name };
  const heapStart = performance.memory?.usedJSHeapSize ?? null;
  for (let index = 0; index < roomChanges; index += 1) {
    draw.draw(performance.now(), (index % 6) * 0.2);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const heapAfterChanges = performance.memory?.usedJSHeapSize ?? null;
  await new Promise((resolve) => setTimeout(resolve, holdMs));
  draw.draw(performance.now(), 0);
  return {
    available: true,
    renderer: name,
    roomChanges,
    holdMs,
    heapStart,
    heapAfterChanges,
    heapAfterHold: performance.memory?.usedJSHeapSize ?? null,
  };
}
