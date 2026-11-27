// The baseline renderer: a hand-written pseudo-3D projection, painter's depth sort and
// flat-shaded rooms and figures. It is a pure function of (scene, time, camera).
import { FIGURES, LABELS, ROOMS, SCALE, figurePosition, project } from "./scene.js";

export function create(canvas) {
  const context = canvas.getContext("2d");
  if (!context) return null;

  return {
    draw(t, cam = 0) {
      context.fillStyle = "#fffaf0";
      context.fillRect(0, 0, canvas.width, canvas.height);
      const items = [];

      for (const room of ROOMS) {
        const center = project(room.x, room.z, cam);
        const width = room.w * SCALE * 0.9;
        const height = room.d * SCALE * 0.55 * 0.9;
        items.push({
          depth: center.depth,
          draw: () => {
            context.fillStyle = room.color;
            context.strokeStyle = "#b99a6b";
            context.lineWidth = 2;
            context.beginPath();
            context.roundRect(center.x - width / 2, center.y - height / 2, width, height, 10);
            context.fill();
            context.stroke();
          },
        });
      }

      for (const figure of FIGURES) {
        const position = figurePosition(figure, t);
        const point = project(position.x, position.z, cam);
        const size = figure.moving ? 11 : 9;
        items.push({
          depth: point.depth,
          draw: () => {
            context.fillStyle = figure.color;
            context.beginPath();
            context.arc(point.x, point.y - size, size, 0, Math.PI * 2);
            context.fill();
            context.fillStyle = "#3b2d1e";
            context.fillRect(point.x - 2, point.y - size * 0.5, 4, size * 0.9);
          },
        });
      }

      items.sort((a, b) => b.depth - a.depth).forEach((item) => item.draw());

      context.fillStyle = "#3b2d1e";
      context.font = "11px ui-rounded, system-ui, sans-serif";
      context.textAlign = "center";
      for (const label of LABELS) {
        const point = project(label.x, label.z, cam);
        context.fillText(label.text, point.x, point.y);
      }
    },
  };
}
