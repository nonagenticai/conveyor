// The canvas-2D adapter (ADR 0023). It draws the FrameView and nothing else: no capability,
// no focus, no text entry. The DOM map and sheets are the work surface.

import { project, spriteRect } from "../frame.js";

const INK = "#1f2a2a";
const PARCHMENT = "#efe4c9";
const TEAL = "#2f8070";

/** @param {string} tint @param {string} fallback */
function colour(tint, fallback) {
  return /^#[0-9a-fA-F]{3,8}$/.test(tint) ? tint : fallback;
}

/**
 * @param {CanvasRenderingContext2D} ctx
 * @param {import("../port.js").FrameView} view
 * @param {import("../port.js").Viewport} viewport
 */
export function draw(ctx, view, viewport) {
  ctx.save();
  ctx.clearRect(0, 0, viewport.width, viewport.height);
  ctx.fillStyle = PARCHMENT;
  ctx.fillRect(0, 0, viewport.width, viewport.height);

  const sprites = [...view.sprites].sort((a, b) => a.position[2] - b.position[2]);
  for (const sprite of sprites) {
    const rect = spriteRect(sprite, view.camera, viewport);
    const highlighted = view.highlight && JSON.stringify(view.highlight) === JSON.stringify(sprite.id);
    if (sprite.kind === "room") {
      ctx.fillStyle = colour(sprite.tint, "#f8efd9");
      ctx.strokeStyle = "rgba(31,42,42,0.18)";
      ctx.lineWidth = 1;
      roundRect(ctx, rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, 10);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = "rgba(31,42,42,0.75)";
      ctx.font = "600 13px ui-sans-serif, system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(sprite.label, rect.centre.x, rect.centre.y + 4);
    } else if (sprite.kind === "wall") {
      ctx.fillStyle = colour(sprite.tint, TEAL);
      roundRect(ctx, rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, 3);
      ctx.fill();
      if (highlighted) {
        ctx.strokeStyle = "#b8892b";
        ctx.lineWidth = 3;
        ctx.stroke();
      }
      if (highlighted) {
        ctx.fillStyle = INK;
        ctx.font = "600 12px ui-sans-serif, system-ui, sans-serif";
        ctx.textAlign = "center";
        ctx.fillText(sprite.label, rect.centre.x, rect.top - 8);
      }
    } else if (sprite.kind === "record") {
      ctx.save();
      ctx.translate(rect.centre.x, rect.centre.y - 6);
      ctx.rotate(Math.PI / 4);
      ctx.fillStyle = colour(sprite.tint, "#8b6c3a");
      ctx.fillRect(-5, -5, 10, 10);
      ctx.restore();
    } else if (sprite.kind === "avatar") {
      ctx.beginPath();
      ctx.arc(rect.centre.x, rect.centre.y - 10, 10, 0, Math.PI * 2);
      ctx.fillStyle = colour(sprite.tint, "#c96f2f");
      ctx.fill();
      ctx.strokeStyle = INK;
      ctx.lineWidth = 1.5;
      ctx.stroke();
      const ahead = project(
        [sprite.position[0] + Math.sin(sprite.facing) * 0.6, sprite.position[1], sprite.position[2] + Math.cos(sprite.facing) * 0.6],
        view.camera,
        viewport,
      );
      ctx.beginPath();
      ctx.moveTo(rect.centre.x, rect.centre.y - 10);
      ctx.lineTo(ahead.x, ahead.y - 10);
      ctx.stroke();
    } else if (sprite.kind === "prompt" || sprite.kind === "mood" || sprite.kind === "badge") {
      ctx.fillStyle = INK;
      ctx.font = "12px ui-sans-serif, system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(sprite.label, rect.centre.x, rect.top - 6);
    }
  }

  ctx.fillStyle = "rgba(31,42,42,0.8)";
  ctx.font = "12px ui-sans-serif, system-ui, sans-serif";
  ctx.textAlign = "left";
  for (const overlay of view.overlays) {
    const point = project(overlay.anchor, view.camera, viewport);
    ctx.fillText(overlay.text, point.x + 8, point.y);
  }
  ctx.restore();
}

/**
 * The no-engine fallback: eight room panels from the JSON, with their destination counts.
 * It is labelled as the fallback in the DOM; the map beside it is the accessible equivalent.
 * @param {CanvasRenderingContext2D} ctx
 * @param {{rooms: Array<{id: string, label: string}>, walls: Array<{room: string}>}} world
 * @param {import("../port.js").Viewport} viewport
 * @returns {Array<{roomId: string, left: number, top: number, right: number, bottom: number}>}
 */
export function drawFallback(ctx, world, viewport) {
  ctx.save();
  ctx.clearRect(0, 0, viewport.width, viewport.height);
  ctx.fillStyle = PARCHMENT;
  ctx.fillRect(0, 0, viewport.width, viewport.height);
  const columns = viewport.width < 520 ? 1 : 2;
  const rows = Math.ceil(world.rooms.length / columns);
  const gap = 12;
  const cellWidth = (viewport.width - gap * (columns + 1)) / columns;
  const cellHeight = Math.max(56, (viewport.height - gap * (rows + 1)) / rows);
  /** @type {Array<{roomId: string, left: number, top: number, right: number, bottom: number}>} */
  const panels = [];
  world.rooms.forEach((room, index) => {
    const column = index % columns;
    const row = Math.floor(index / columns);
    const left = gap + column * (cellWidth + gap);
    const top = gap + row * (cellHeight + gap);
    ctx.fillStyle = "#f8efd9";
    ctx.strokeStyle = "rgba(31,42,42,0.25)";
    ctx.lineWidth = 1;
    roundRect(ctx, left, top, cellWidth, cellHeight, 10);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = INK;
    ctx.font = "600 13px ui-sans-serif, system-ui, sans-serif";
    ctx.textAlign = "left";
    ctx.fillText(room.label, left + 12, top + 24);
    const count = world.walls.filter((wall) => wall.room === room.id).length;
    ctx.fillStyle = "rgba(31,42,42,0.65)";
    ctx.font = "12px ui-sans-serif, system-ui, sans-serif";
    ctx.fillText(`${count} destination${count === 1 ? "" : "s"}`, left + 12, top + 44);
    panels.push({ roomId: room.id, left, top, right: left + cellWidth, bottom: top + cellHeight });
  });
  ctx.restore();
  return panels;
}

/** @param {CanvasRenderingContext2D} ctx @param {number} x @param {number} y @param {number} w @param {number} h @param {number} r */
function roundRect(ctx, x, y, w, h, r) {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + w, y, x + w, y + h, radius);
  ctx.arcTo(x + w, y + h, x, y + h, radius);
  ctx.arcTo(x, y + h, x, y, radius);
  ctx.arcTo(x, y, x + w, y, radius);
  ctx.closePath();
}
