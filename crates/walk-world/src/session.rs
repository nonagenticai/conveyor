//! Session truth: the seat, the sheet, the selection and the camera. It is the only state
//! `save()` serialises; the mirror is never round-tripped.

use serde::{Deserialize, Serialize};

use crate::component::CameraMode;
use crate::entity::{InteractableId, SeatId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraSnapshot {
    pub mode: CameraMode,
    pub distance: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub seat: SeatId,
    pub sheet: Option<InteractableId>,
    pub selection: Option<InteractableId>,
    pub camera: CameraSnapshot,
}
