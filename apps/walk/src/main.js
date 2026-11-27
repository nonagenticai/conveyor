// Bootstrap: config -> auth -> manifest -> router/shell -> world. A failure before the shell
// exists is shown as text in the screen region, never as a blank page.

import { loadConfig } from "./auth/config.js";
import { Auth } from "./auth/index.js";
import { Api } from "./api/client.js";
import { routes } from "./router/registry.js";
import { Shell } from "./shell/shell.js";
import { World } from "./world/index.js";

async function main() {
  const config = await loadConfig();
  const auth = new Auth(config);
  await auth.boot();
  const api = new Api(auth);
  const worldData = await fetch("/data/world.json", { cache: "no-store" }).then((response) => {
    if (!response.ok) throw new Error(`world.json answered ${response.status}`);
    return response.json();
  });
  const canvas = /** @type {HTMLCanvasElement} */ (document.getElementById("world-canvas"));
  /** @type {Shell} */
  let shell;
  const world = new World({
    canvas,
    worldData,
    onWall: (wall) => shell.openWall(wall),
    onRoom: (roomId) => shell.openRoom(roomId),
  });
  shell = new Shell({ auth, api, routes, worldData, world });
  await shell.start();
}

main().catch((error) => {
  const screen = document.getElementById("screen");
  if (screen) {
    screen.textContent = `The walk could not start: ${error instanceof Error ? error.message : String(error)}`;
  }
});
