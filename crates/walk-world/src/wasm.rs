//! The wasm boundary: JSON at the edge, serde inside. The bindings are compiled only for the
//! `wasm` feature on wasm32; the JSON functions are also the native parity surface the tests use.

use crate::command::Outcome;
use crate::entity::InteractableId;
use crate::message::Input;
use crate::projection::{Delta, Snapshot};
use crate::resource::{ActionManifest, WorldConfig};
use crate::session::SessionSnapshot;
use crate::view::SheetContext;
use crate::{FrameView, WalkWorld};

pub fn new_json(seed: u64, config_json: &str) -> Result<WalkWorld, String> {
    let mut config: WorldConfig =
        serde_json::from_str(config_json).map_err(|error| error.to_string())?;
    config.seed = seed;
    Ok(WalkWorld::new(config))
}

pub fn advance_json(world: &mut WalkWorld, dt_ms: f64) {
    world.advance(std::time::Duration::from_secs_f64(
        (dt_ms / 1000.0).max(0.0),
    ));
}

pub fn push_input_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let input: Input = serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.push_input(input);
    Ok(())
}

pub fn push_snapshot_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let snapshot: Snapshot = serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.push_snapshot(&snapshot);
    Ok(())
}

pub fn push_delta_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let delta: Delta = serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.push_delta(&delta);
    Ok(())
}

pub fn resolve_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let receipt: crate::message::ReceiptIn =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.resolve(receipt.id, receipt.outcome);
    Ok(())
}

pub fn set_manifest_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let manifest: ActionManifest = serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.set_manifest(manifest);
    Ok(())
}

pub fn drain_calls_json(world: &mut WalkWorld) -> String {
    serde_json::to_string(&world.drain_calls()).unwrap_or_else(|_| "[]".into())
}

pub fn view_json(world: &WalkWorld) -> String {
    serde_json::to_string(&world.view()).unwrap_or_else(|_| "{}".into())
}

pub fn hash_json(world: &WalkWorld) -> String {
    crate::hash_view(&world.view())
}

pub fn context_json(world: &WalkWorld, interactable_json: &str) -> Result<String, String> {
    let target: InteractableId =
        serde_json::from_str(interactable_json).map_err(|error| error.to_string())?;
    let context: SheetContext = world.context(target);
    serde_json::to_string(&context).map_err(|error| error.to_string())
}

pub fn save_json(world: &WalkWorld) -> String {
    serde_json::to_string(&world.save()).unwrap_or_else(|_| "{}".into())
}

pub fn load_json(world: &mut WalkWorld, json: &str) -> Result<(), String> {
    let snapshot: SessionSnapshot =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    world.load(&snapshot);
    Ok(())
}

/// The exported view type for the tests: a stable alias for the JSON view.
pub type ViewJson = FrameView;

/// The wasm-bindgen surface (ECS §11). Built only with `--features wasm` on wasm32.
#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
mod bindings {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmWalkWorld {
        inner: WalkWorld,
    }

    #[wasm_bindgen]
    impl WasmWalkWorld {
        #[wasm_bindgen(constructor)]
        pub fn new(seed: u64, config_json: &str) -> Result<WasmWalkWorld, JsValue> {
            console_error_panic_hook::set_once();
            new_json(seed, config_json)
                .map(|inner| WasmWalkWorld { inner })
                .map_err(|error| JsValue::from_str(&error))
        }

        pub fn advance(&mut self, dt_ms: f64) {
            advance_json(&mut self.inner, dt_ms);
        }

        pub fn push_input(&mut self, json: &str) -> Result<(), JsValue> {
            push_input_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn push_snapshot(&mut self, json: &str) -> Result<(), JsValue> {
            push_snapshot_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn push_delta(&mut self, json: &str) -> Result<(), JsValue> {
            push_delta_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn resolve(&mut self, json: &str) -> Result<(), JsValue> {
            resolve_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn set_manifest(&mut self, json: &str) -> Result<(), JsValue> {
            set_manifest_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn drain_calls(&mut self) -> String {
            drain_calls_json(&mut self.inner)
        }

        pub fn view(&self) -> String {
            view_json(&self.inner)
        }

        pub fn view_hash(&self) -> String {
            hash_json(&self.inner)
        }

        pub fn context(&self, interactable_json: &str) -> Result<String, JsValue> {
            context_json(&self.inner, interactable_json).map_err(|error| JsValue::from_str(&error))
        }

        pub fn save(&self) -> String {
            save_json(&self.inner)
        }

        pub fn load(&mut self, json: &str) -> Result<(), JsValue> {
            load_json(&mut self.inner, json).map_err(|error| JsValue::from_str(&error))
        }
    }
}

/// A type-level check that the wire types stay serialisable at the boundary.
pub fn _wire_types_are_json() {
    fn assert_serializable<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
    assert_serializable::<Input>();
    assert_serializable::<Snapshot>();
    assert_serializable::<Delta>();
    assert_serializable::<Outcome>();
    assert_serializable::<InteractableId>();
}
