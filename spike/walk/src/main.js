// The spike's entry point: it exposes the proofs the Playwright spec drives and wires the
// buttons for a human run. Nothing here survives W1; the record and ADR 0023 do.
import * as api from "./api.js";
import * as auth from "./auth.js";
import * as measure from "./measure.js";
import * as schema from "./schema.js";
import * as sse from "./sse.js";

const state = { config: null, me: null, fixture: null, sse: null, results: {} };
const canvas = document.getElementById("world");
const formsHost = document.getElementById("forms");
const logEl = document.getElementById("log");
const statusEl = document.getElementById("status");

function log(line) {
  logEl.textContent += `${line}\n`;
}

async function boot() {
  state.config = await auth.loadConfig();
  try {
    state.me = await api.me();
  } catch (error) {
    state.me = { error: error.message, status: error.status };
  }
  const name = state.me?.principal?.display_name;
  statusEl.textContent = name ? `signed in as ${name}` : `not authenticated: ${state.me?.error ?? "unknown"}`;
  requestAnimationFrame(() => performance.mark("spike-first-frame"));
}

async function describeForms() {
  const names = ["programmes.list", "programmes.update", "cards.archive"];
  const out = {};
  formsHost.innerHTML = "";
  for (const name of names) {
    const description = await api.describe(name);
    const form = schema.renderForm(description.input, {});
    const section = document.createElement("section");
    section.className = "sheet";
    const title = document.createElement("h2");
    title.textContent = `${name} · ${description.effect} · ${description.confirm}`;
    section.append(title, form);
    formsHost.append(section);
    out[name] = {
      fields: schema.fields(description.input),
      effect: description.effect,
      confirm: description.confirm,
      tenancy: description.tenancy,
    };
  }
  return out;
}

window.__spike = {
  configure(fixture) {
    state.fixture = fixture;
  },
  me: () => state.me,
  config: () => state.config,
  trace: api.trace,
  pkce: (config) => auth.pkce(config ?? state.config ?? {}),
  describe: (name) => api.describe(name),
  forms: describeForms,
  read: async () => {
    const { output } = await api.invoke("programmes.list", { limit: 200 });
    state.results.read = output;
    return output;
  },
  moveCard: async () => {
    const fixture = state.fixture;
    if (!fixture) throw new Error("no fixture configured");
    const { output } = await api.invoke(
      "cards.move",
      { id: fixture.cardId, list_id: fixture.activeListId },
      { ifMatch: fixture.version },
    );
    state.results.move = output;
    return { card: output };
  },
  createCard: async (name) => {
    const fixture = state.fixture;
    const key = crypto.randomUUID();
    const { output } = await api.invoke(
      "cards.create",
      {
        board_id: fixture.boardId,
        list_id: fixture.intakeListId,
        name,
        card_type_code: "bug",
      },
      { idempotencyKey: key },
    );
    state.results.create = { card: output, idempotencyKey: key };
    return state.results.create;
  },
  archive: async () => {
    const fixture = state.fixture;
    try {
      const { output } = await api.invoke(
        "cards.archive",
        { id: fixture.cardId },
        { ifMatch: fixture.version },
      );
      return { ok: true, output };
    } catch (error) {
      return { ok: false, status: error.status, problem: error.problem, message: error.message };
    }
  },
  subscribe: async () => {
    const fixture = state.fixture;
    state.sse?.close();
    state.sse = await sse.subscribe([`board:${fixture.boardId}`], (frame) =>
      log(`sse ${frame.event ?? frame.data?.type ?? "frame"}`),
    );
    return state.sse.frames.length;
  },
  sseFrames: () => state.sse?.frames ?? [],
  measure: (name, seconds = 5) => measure.measure(name, canvas, seconds),
  soak: (name) => measure.soak(name, canvas),
  results: () => state.results,
};

document.querySelectorAll("button[data-proof]").forEach((button) => {
  button.addEventListener("click", async () => {
    const proof = button.dataset.proof;
    try {
      let result;
      if (proof === "read") result = await window.__spike.read();
      else if (proof === "write") result = await window.__spike.moveCard();
      else if (proof === "destructive") result = await window.__spike.archive();
      else if (proof === "sse") result = await window.__spike.subscribe();
      else if (proof === "pkce") result = await window.__spike.pkce();
      else if (proof === "forms") result = await window.__spike.forms();
      else if (proof === "measure-canvas") result = await window.__spike.measure("canvas2d", 3);
      else if (proof === "measure-webgl") result = await window.__spike.measure("webgl2", 3);
      log(`${proof}: ${JSON.stringify(result).slice(0, 600)}`);
    } catch (error) {
      log(`${proof} failed: ${error.message}`);
    }
  });
});

boot();
