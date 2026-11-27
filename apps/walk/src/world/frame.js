// Pure projection and picking for the canvas adapter. No DOM, no engine: the test can time and
// assert it without a browser.

/**
 * Project a world position into canvas pixels. The camera is the engine's, a simple
 * orthographic-ish projection keeps the walk readable at every zoom.
 * @param {[number, number, number]} position
 * @param {import("./port.js").CameraView} camera
 * @param {import("./port.js").Viewport} viewport
 * @returns {{x: number, y: number, depth: number}}
 */
export function project(position, camera, viewport) {
  const [x, y, z] = position;
  const [cx, , cz] = camera.position;
  return {
    x: viewport.width / 2 + (x - cx) * viewport.scale,
    y: viewport.height * 0.62 - (z - cz) * viewport.scale - y * viewport.scale,
    depth: z,
  };
}

/**
 * The screen rect a sprite occupies.
 * @param {import("./port.js").SpriteView} sprite
 * @param {import("./port.js").CameraView} camera
 * @param {import("./port.js").Viewport} viewport
 */
export function spriteRect(sprite, camera, viewport) {
  const centre = project(sprite.position, camera, viewport);
  const width = Math.max(6, sprite.size[0] * viewport.scale);
  const height = Math.max(6, sprite.size[1] * viewport.scale);
  return {
    left: centre.x - width / 2,
    top: centre.y - height,
    right: centre.x + width / 2,
    bottom: centre.y,
    centre,
  };
}

/**
 * The topmost sprite with an id under a point, or null. Sprites are drawn in array order, so
 * the last hit wins — the same rule the canvas uses.
 * @param {import("./port.js").FrameView} view
 * @param {number} x
 * @param {number} y
 * @param {import("./port.js").Viewport} viewport
 * @returns {import("./port.js").SpriteView|null}
 */
export function spriteAt(view, x, y, viewport) {
  let hit = null;
  for (const sprite of view.sprites) {
    if (!sprite.id) continue;
    const rect = spriteRect(sprite, view.camera, viewport);
    // A generous vertical band: walls are thin posts but must remain easy to click.
    if (
      x >= rect.left &&
      x <= rect.right &&
      y >= rect.top - 10 &&
      y <= rect.bottom + 10
    ) {
      hit = sprite;
    }
  }
  return hit;
}

/**
 * Fit a scale so a room ring of `radius` world units is visible.
 * @param {number} width
 * @param {number} height
 * @param {number} [radius]
 */
export function fitScale(width, height, radius = 9) {
  return Math.max(18, Math.min(width, height) / (radius * 2));
}
