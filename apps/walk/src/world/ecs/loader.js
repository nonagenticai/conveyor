// The wasm engine loader. `wasm-bindgen --target web` exports a default init function and the
// `WasmWalkWorld` class. A failed load is not fatal: the shell keeps the DOM map, and the
// canvas draws a labelled fallback.

export async function loadEngine() {
  // A URL specifier keeps the generated package out of the type graph: pkg/ is built by the
  // image and CI, never committed, and wasm-bindgen's glue is not written for tsc.
  const url = new URL("../../../pkg/walk_world.js", import.meta.url);
  const module = await import(url.href);
  await module.default();
  return module;
}
