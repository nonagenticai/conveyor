// The world entrance: the canvas, the rAF loop, the input bridge and the fallback. The
// simulation is `crates/walk-world` (bevy_ecs 0.20.1) compiled to wasm; the canvas adapter
// draws its FrameView. When the engine is unavailable, the room panels are drawn from the
// JSON and every capability stays in the DOM map (SPEC §8).

import { draw, drawFallback } from "./canvas2d/renderer.js";
import { fitScale, spriteAt } from "./frame.js";
import { loadEngine } from "./ecs/loader.js";
import { roomIndexOf, wallIndexOf } from "./port.js";

const TIMESTEP_MS = 16.666667;
const MOVE_KEYS = new Set(["w", "a", "s", "d", "arrowup", "arrowdown", "arrowleft", "arrowright"]);

export class World {
  /**
   * @param {{canvas: HTMLCanvasElement, worldData: any,
   *          onWall: (wall: any) => void, onRoom: (roomId: string) => void}} deps
   */
  constructor({ canvas, worldData, onWall, onRoom }) {
    this.canvas = canvas;
    this.ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
    this.worldData = worldData;
    this.onWall = onWall;
    this.onRoom = onRoom;
    this.engine = null;
    this.view = null;
    this.panels = [];
    this.zoom = 1;
    this.keys = new Set();
    this.started = false;
    this.last = 0;
    this.width = 0;
    this.height = 0;
    this.onResize = () => this.resize();
    this.loop = (time) => this.frame(time);
    this.onKeyDown = (event) => this.keyDown(event);
    this.onKeyUp = (event) => this.keyUp(event);
    this.onPointer = (event) => this.pointer(event);
    this.onWheel = (event) => this.wheel(event);
  }

  async mount() {
    if (this.started) return;
    this.started = true;
    this.resize();
    window.addEventListener("resize", this.onResize);
    this.canvas.addEventListener("pointerdown", this.onPointer);
    this.canvas.addEventListener("wheel", this.onWheel, { passive: false });
    window.addEventListener("keydown", this.onKeyDown);
    window.addEventListener("keyup", this.onKeyUp);
    await this.startEngine();
    requestAnimationFrame(this.loop);
  }

  config() {
    return {
      rooms: this.worldData.rooms.map((room) => ({ id: room.id, label: room.label })),
      walls: this.worldData.walls.map((wall) => ({
        route: wall.walk,
        room: wall.room,
        label: wall.view,
        view: wall.view,
      })),
    };
  }

  async startEngine() {
    try {
      const module = await loadEngine();
      this.engine = new module.WasmWalkWorld(7n, JSON.stringify(this.config()));
      this.canvas.dataset.engine = "ecs";
      this.canvas.setAttribute(
        "aria-label",
        "Walk world: the rooms and their destination walls. The room list below is the keyboard path.",
      );
    } catch {
      this.engine = null;
      this.canvas.dataset.engine = "fallback";
      this.canvas.setAttribute(
        "aria-label",
        "Walk world (fallback): room panels drawn without the engine. The room list below is the keyboard path.",
      );
    }
  }

  resize() {
    const parent = this.canvas.parentElement;
    const width = Math.max(320, parent ? parent.clientWidth : window.innerWidth);
    const height = Math.max(260, parent ? parent.clientHeight : 360);
    const dpr = window.devicePixelRatio || 1;
    this.width = width;
    this.height = height;
    this.canvas.width = Math.round(width * dpr);
    this.canvas.height = Math.round(height * dpr);
    this.canvas.style.width = `${width}px`;
    this.canvas.style.height = `${height}px`;
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  viewport() {
    return { width: this.width, height: this.height, scale: fitScale(this.width, this.height) * this.zoom };
  }

  frame(time) {
    if (!this.started) return;
    const dt = this.last === 0 ? TIMESTEP_MS : Math.min(80, time - this.last);
    this.last = time;
    const viewport = this.viewport();
    if (this.engine) {
      if (this.keys.size > 0) this.pushMove();
      this.engine.advance(dt);
      try {
        this.view = JSON.parse(this.engine.view());
        draw(this.ctx, this.view, viewport);
      } catch (error) {
        this.engine = null;
        this.canvas.dataset.engine = "fallback";
      }
    } else {
      this.panels = drawFallback(this.ctx, this.worldData, viewport);
    }
    requestAnimationFrame(this.loop);
  }

  pushMove() {
    let x = 0;
    let z = 0;
    if (this.keys.has("a") || this.keys.has("arrowleft")) x -= 1;
    if (this.keys.has("d") || this.keys.has("arrowright")) x += 1;
    if (this.keys.has("w") || this.keys.has("arrowup")) z -= 1;
    if (this.keys.has("s") || this.keys.has("arrowdown")) z += 1;
    const length = Math.hypot(x, z) || 1;
    this.push({ kind: "move", data: { x: x / length, z: z / length } });
  }

  /** @param {any} input */
  push(input) {
    if (!this.engine) return;
    try {
      this.engine.push_input(JSON.stringify(input));
    } catch {
      // An input the engine refuses is dropped; it must never break the frame loop.
    }
  }

  keyDown(event) {
    const key = event.key.toLowerCase();
    if (key === "v" && this.engine) {
      this.push({
        kind: "set_camera",
        data: { mode: this.canvas.dataset.camera === "first" ? "Third" : "First" },
      });
      this.canvas.dataset.camera = this.canvas.dataset.camera === "first" ? "third" : "first";
      return;
    }
    if (!MOVE_KEYS.has(key)) return;
    // Arrows scroll the page otherwise, and the world owns them while focused.
    if (key.startsWith("arrow")) event.preventDefault();
    this.keys.add(key);
  }

  keyUp(event) {
    const key = event.key.toLowerCase();
    if (!MOVE_KEYS.has(key)) return;
    this.keys.delete(key);
    if (this.keys.size === 0) this.push({ kind: "stop" });
  }

  /** @param {PointerEvent} event */
  pointer(event) {
    const rect = this.canvas.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (this.engine && this.view) {
      const sprite = spriteAt(this.view, x, y, this.viewport());
      if (!sprite) return;
      const wallIndex = wallIndexOf(sprite.id);
      if (wallIndex !== null && this.worldData.walls[wallIndex]) {
        this.onWall(this.worldData.walls[wallIndex]);
        return;
      }
      const roomIndex = roomIndexOf(sprite.id);
      if (roomIndex !== null && this.worldData.rooms[roomIndex]) {
        this.onRoom(this.worldData.rooms[roomIndex].id);
      }
      return;
    }
    const panel = this.panels.find(
      (item) => x >= item.left && x <= item.right && y >= item.top && y <= item.bottom,
    );
    if (panel) this.onRoom(panel.roomId);
  }

  /** @param {WheelEvent} event */
  wheel(event) {
    event.preventDefault();
    this.zoom = Math.min(1.8, Math.max(0.6, this.zoom + (event.deltaY > 0 ? -0.08 : 0.08)));
  }
}
