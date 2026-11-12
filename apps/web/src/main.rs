//! The bin target cargo-leptos requires to resolve this package. It is not the web entry
//! point: the browser starts at the lib's `#[wasm_bindgen(start)]`, and the build uses
//! `--frontend-only`, so nothing here is shipped.

fn main() {}
