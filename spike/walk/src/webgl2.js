// The candidate renderer: the same FrameView-shaped scene through WebGL2. It exists to be
// measured against canvas-2D, not to draw better; the ADR records the winner.
import { FIGURES, ROOMS, SCALE, figurePosition, hexColor, project } from "./scene.js";

const VERTEX = `#version 300 es
in vec2 a_position;
in vec3 a_color;
out vec3 v_color;
void main() {
  gl_Position = vec4(a_position, 0.0, 1.0);
  v_color = a_color;
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
in vec3 v_color;
out vec4 out_color;
void main() {
  out_color = vec4(v_color, 1.0);
}`;

export function create(canvas) {
  const gl = canvas.getContext("webgl2", { antialias: false, alpha: false });
  if (!gl) return null;

  const compile = (type, source) => {
    const shader = gl.createShader(type);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(gl.getShaderInfoLog(shader) ?? "shader compile failed");
    }
    return shader;
  };

  const program = gl.createProgram();
  gl.attachShader(program, compile(gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAGMENT));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? "program link failed");
  }
  gl.useProgram(program);

  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  const position = gl.getAttribLocation(program, "a_position");
  const color = gl.getAttribLocation(program, "a_color");
  gl.enableVertexAttribArray(position);
  gl.enableVertexAttribArray(color);
  gl.viewport(0, 0, canvas.width, canvas.height);

  const data = [];

  function push(x, y, rgb) {
    data.push((x / canvas.width) * 2 - 1, 1 - (y / canvas.height) * 2, rgb[0], rgb[1], rgb[2]);
  }

  function quad(x0, y0, x1, y1, rgb) {
    push(x0, y0, rgb);
    push(x1, y0, rgb);
    push(x1, y1, rgb);
    push(x0, y0, rgb);
    push(x1, y1, rgb);
    push(x0, y1, rgb);
  }

  return {
    draw(t, cam = 0) {
      data.length = 0;
      for (const room of ROOMS) {
        const center = project(room.x, room.z, cam);
        const width = room.w * SCALE * 0.9;
        const height = room.d * SCALE * 0.55 * 0.9;
        quad(
          center.x - width / 2,
          center.y - height / 2,
          center.x + width / 2,
          center.y + height / 2,
          hexColor(room.color),
        );
      }
      for (const figure of FIGURES) {
        const position = figurePosition(figure, t);
        const point = project(position.x, position.z, cam);
        const size = figure.moving ? 11 : 9;
        quad(point.x - size, point.y - size * 2, point.x + size, point.y, hexColor(figure.color));
      }

      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.DYNAMIC_DRAW);
      gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 20, 0);
      gl.vertexAttribPointer(color, 3, gl.FLOAT, false, 20, 8);
      gl.clearColor(1, 0.98, 0.94, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.drawArrays(gl.TRIANGLES, 0, data.length / 5);
    },
  };
}
