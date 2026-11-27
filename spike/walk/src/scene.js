// The reference scene from SPEC §12: six rooms, twelve figures, labels, one moving figure.
// It is not the app's world; it is the measurement fixture both renderers consume.
export const SCALE = 105;
export const OX = 360;
export const OY = 250;

export const ROOMS = [
  { id: "atelier", label: "Architecture atelier", x: -2.4, z: -1.6, w: 2.2, d: 1.6, color: "#e7d3b0" },
  { id: "programme", label: "Programme office", x: 0, z: -1.6, w: 2.2, d: 1.6, color: "#d9e4cd" },
  { id: "analysis", label: "Analysis library", x: 2.4, z: -1.6, w: 2.2, d: 1.6, color: "#e8c9b8" },
  { id: "delivery", label: "Delivery studio", x: -2.4, z: 1.6, w: 2.2, d: 1.6, color: "#cfe0e6" },
  { id: "flow", label: "Flow lab", x: 0, z: 1.6, w: 2.2, d: 1.6, color: "#e6d1e0" },
  { id: "kata", label: "Kata studio", x: 2.4, z: 1.6, w: 2.2, d: 1.6, color: "#dfe6c8" },
];

export const FIGURES = Array.from({ length: 12 }, (_, index) => {
  const room = ROOMS[index % ROOMS.length];
  return {
    id: `figure-${index}`,
    x: room.x + (index % 2 === 0 ? -0.45 : 0.45),
    z: room.z + 0.25,
    color: index === 0 ? "#8fd14f" : "#2f8070",
    moving: index === 0,
  };
});

export const LABELS = ROOMS.map((room) => ({
  x: room.x,
  z: room.z - room.d / 2 - 0.35,
  text: room.label,
}));

export function project(x, z, cam = 0) {
  const cos = Math.cos(cam);
  const sin = Math.sin(cam);
  const rx = x * cos - z * sin;
  const rz = x * sin + z * cos;
  return { x: OX + rx * SCALE, y: OY + rz * SCALE * 0.55, depth: rz };
}

export function hexColor(hex) {
  const value = parseInt(hex.slice(1), 16);
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}

export function figurePosition(figure, t) {
  return {
    x: figure.x + (figure.moving ? Math.sin(t / 650) * 0.9 : 0),
    z: figure.z,
  };
}
