// The WorldView port (SPEC §6.2, ECS §13). The shell and the canvas consume this shape only;
// the simulation behind it (bevy_ecs in wasm) is replaceable, and a canvas-2D fallback can
// draw the same view.

/**
 * @typedef {{mode: string, position: [number, number, number], target: [number, number, number]}} CameraView
 * @typedef {{id?: any, kind: string, position: [number, number, number], size: [number, number],
 *            facing: number, tint: string, label: string}} SpriteView
 * @typedef {{id?: any, kind: string, anchor: [number, number, number], text: string}} OverlayView
 * @typedef {{camera: CameraView, sprites: SpriteView[], overlays: OverlayView[],
 *            highlight?: any, room?: any}} FrameView
 * @typedef {{width: number, height: number, scale: number}} Viewport
 */

/**
 * The wall index out of an InteractableId, or null when it is not a wall.
 * @param {any} id
 * @returns {number|null}
 */
export function wallIndexOf(id) {
  if (id && typeof id === "object" && Number.isInteger(id.Wall)) return id.Wall;
  return null;
}

/**
 * The room index out of an InteractableId, or null when it is not a room.
 * @param {any} id
 * @returns {number|null}
 */
export function roomIndexOf(id) {
  if (id && typeof id === "object" && Number.isInteger(id.Room)) return id.Room;
  return null;
}
