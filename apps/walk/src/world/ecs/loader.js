// The wasm engine loader. `wasm-bindgen --target web` exports a default init function and the
// `WasmWalkWorld` class. A failed load is not fatal: the shell keeps the DOM map, and the
// canvas draws a labelled fallback.

export async function loadEngine() {
  // @ts-expect-error pkg/ is built by the image/CI, never committed; it has no typings.
  const module = await import("../../../pkg/walk_world.js");
  await module.default();
  return module;
}
