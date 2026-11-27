// JSON Schema -> labelled fields. A schema the renderer cannot express falls back to a
// labelled JSON editor, so no action is unreachable (SPEC §6, plan §6). Server strings are
// always text; nothing here interpolates into HTML.

import { el, field } from "../dom.js";
import { ProblemError, problemText } from "../api/problem.js";

/**
 * @typedef {{name: string, kind: "string"|"number"|"integer"|"boolean"|"enum"|"json",
 *            label: string, hint: string, required: boolean, options?: string[],
 *            minimum?: number, maximum?: number}} Field
 */

/** @param {string} name */
export function humanise(name) {
  const spaced = name.replace(/[_-]+/g, " ").replace(/([a-z])([A-Z])/g, "$1 $2");
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/**
 * The fields an object schema asks for, in declaration order.
 * @param {any} schema
 * @returns {Field[]}
 */
export function fieldsFromSchema(schema) {
  if (!schema || typeof schema !== "object") return [];
  const properties = schema.properties;
  if (!properties || typeof properties !== "object" || Array.isArray(properties)) return [];
  const required = new Set(Array.isArray(schema.required) ? schema.required : []);
  /** @type {Field[]} */
  const fields = [];
  for (const [name, raw] of Object.entries(properties)) {
    const property = raw && typeof raw === "object" ? raw : {};
    /** @type {Field} */
    const item = {
      name,
      kind: "json",
      label: typeof property.title === "string" ? property.title : humanise(name),
      hint: typeof property.description === "string" ? property.description : "",
      required: required.has(name),
    };
    // An optional field (Rust `Option<T>`) is `{"type": ["string", "null"]}`, and an optional
    // enum can arrive as a `oneOf`/`anyOf` of the value and null. Pick the single non-null
    // shape; anything still composite is the JSON fallback.
    let shape = property;
    if (Array.isArray(property.oneOf) || Array.isArray(property.anyOf)) {
      const variants = property.oneOf ?? property.anyOf;
      const nonNull = variants.filter((item) => item && item.type !== "null");
      if (nonNull.length === 1) shape = nonNull[0];
    }
    const type = Array.isArray(shape.type)
      ? shape.type.find((item) => item !== "null")
      : shape.type;
    const enumValues = Array.isArray(shape.enum) ? shape.enum : property.enum;
    if (Array.isArray(enumValues) && enumValues.length > 0) {
      item.kind = "enum";
      item.options = enumValues.map((value) => String(value));
    } else if (type === "string") {
      item.kind = "string";
    } else if (type === "integer") {
      item.kind = "integer";
      if (typeof shape.minimum === "number") item.minimum = shape.minimum;
      if (typeof shape.maximum === "number") item.maximum = shape.maximum;
    } else if (type === "number") {
      item.kind = "number";
      if (typeof shape.minimum === "number") item.minimum = shape.minimum;
      if (typeof shape.maximum === "number") item.maximum = shape.maximum;
    } else if (type === "boolean") {
      item.kind = "boolean";
    }
    fields.push(item);
  }
  return fields;
}

/**
 * Render an action input form. `onSubmit` receives the collected object only when every
 * field parses; a parse error is shown beside the field and nothing is submitted.
 * @param {any} schema
 * @param {{submitLabel?: string, initial?: Record<string, unknown>,
 *          onSubmit: (value: Record<string, unknown>) => Promise<void>|void}} options
 * @returns {{element: HTMLFormElement, setBusy: (busy: boolean) => void, error: (message: string) => void}}
 */
export function renderSchemaForm(schema, options) {
  const fields = fieldsFromSchema(schema);
  /** @type {Map<string, {field: Field, input: HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement, error: HTMLElement}>} */
  const controls = new Map();
  const form = /** @type {HTMLFormElement} */ (
    el("form", { class: "action-form", novalidate: true })
  );

  if (fields.length === 0 && !schema?.properties) {
    form.append(
      el("p", { class: "field-hint", text: "This action takes no input." }),
    );
  }

  for (const item of fields) {
    /** @type {HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement} */
    let input;
    if (item.kind === "enum" && item.options) {
      input = /** @type {HTMLSelectElement} */ (el("select", { name: item.name }));
      input.append(el("option", { value: "", text: "—" }));
      for (const option of item.options) {
        input.append(el("option", { value: option, text: option }));
      }
    } else if (item.kind === "boolean") {
      input = /** @type {HTMLInputElement} */ (
        el("input", { type: "checkbox", name: item.name })
      );
    } else if (item.kind === "integer" || item.kind === "number") {
      input = /** @type {HTMLInputElement} */ (
        el("input", {
          type: "number",
          name: item.name,
          step: item.kind === "integer" ? "1" : "any",
          min: item.minimum,
          max: item.maximum,
        })
      );
    } else if (item.kind === "string") {
      input = /** @type {HTMLInputElement} */ (el("input", { type: "text", name: item.name }));
    } else {
      input = /** @type {HTMLTextAreaElement} */ (
        el("textarea", { name: item.name, rows: 4, spellcheck: false })
      );
      if (!item.hint) item.hint = "JSON value";
    }
    input.required = item.required;
    const initial = options.initial?.[item.name];
    if (initial !== undefined && initial !== null) {
      if (item.kind === "boolean") {
        /** @type {HTMLInputElement} */ (input).checked = initial === true;
      } else {
        input.value = String(initial);
      }
    }
    const error = el("p", { class: "field-error", role: "alert" });
    const wrapper = field(item.label + (item.required ? " *" : ""), input, {
      hint: item.hint,
    });
    wrapper.append(error);
    controls.set(item.name, { field: item, input, error });
    form.append(wrapper);
  }

  const submit = /** @type {HTMLButtonElement} */ (
    el("button", { type: "submit", class: "button primary", text: options.submitLabel ?? "Submit" })
  );
  const general = el("p", { class: "form-error", role: "alert" });
  form.append(el("div", { class: "form-actions" }, submit), general);

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    general.textContent = "";
    /** @type {Record<string, unknown>} */
    const value = {};
    let valid = true;
    for (const { field: item, input, error } of controls.values()) {
      error.textContent = "";
      const checkbox = /** @type {HTMLInputElement} */ (input);
      const raw = item.kind === "boolean" ? String(checkbox.checked) : input.value;
      if (item.kind !== "boolean" && raw === "") {
        if (item.required) {
          error.textContent = "required";
          valid = false;
        }
        continue;
      }
      try {
        if (item.kind === "boolean") value[item.name] = checkbox.checked;
        else if (item.kind === "integer") value[item.name] = Number.parseInt(raw, 10);
        else if (item.kind === "number") value[item.name] = Number(raw);
        else if (item.kind === "json") value[item.name] = JSON.parse(raw);
        else value[item.name] = raw;
      } catch {
        error.textContent = "not valid JSON";
        valid = false;
      }
    }
    if (!valid) return;
    submit.disabled = true;
    try {
      await options.onSubmit(value);
    } catch (error) {
      general.textContent =
        error instanceof ProblemError
          ? problemText(error.problem)
          : error instanceof Error
            ? error.message
            : String(error);
    } finally {
      submit.disabled = false;
    }
  });

  return {
    element: form,
    setBusy(busy) {
      submit.disabled = busy;
    },
    error(message) {
      general.textContent = message;
    },
  };
}
