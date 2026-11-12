//! The `conveyor` web UI.
//!
//! S01 decided Leptos CSR/WASM (`docs/decisions/0001-ui-stack.md`); the screens are S12's.
//! One rule from the design is mechanical, not stylistic: **the action registry is the UI's
//! contract**. Every control names an action from `GET /api/actions`; a screen that needs a
//! new action is a contract change, never a local fetch.
//!
//! The crate is a `cdylib` for cargo-leptos and an `rlib` for the host target. Only the
//! wasm32 modules are compiled into the browser; the host build deliberately sees an empty
//! crate, which is what keeps `cargo test --workspace` green (measured on m1, 2026-11-09).
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod api;
#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod auth;
#[cfg(target_arch = "wasm32")]
mod components;
#[cfg(target_arch = "wasm32")]
mod models;
#[cfg(target_arch = "wasm32")]
mod palette;
#[cfg(target_arch = "wasm32")]
mod screens;
#[cfg(target_arch = "wasm32")]
mod sse;
#[cfg(target_arch = "wasm32")]
mod state;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
