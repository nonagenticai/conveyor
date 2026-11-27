// A minimal JSON-Schema form renderer, exactly the contract the W2 sheet generalises: render
// the fields the schema declares, and fall back to a labelled JSON editor so no action is
// unreachable because of a widget.
export function fields(schema) {
  return Object.keys(schema?.properties ?? {});
}

export function renderForm(schema, value = {}) {
  const form = document.createElement("form");
  const properties = schema?.properties ?? {};
  const required = new Set(schema?.required ?? []);

  if (schema?.type !== "object" || Object.keys(properties).length === 0) {
    const area = document.createElement("textarea");
    area.name = "__json";
    area.rows = 6;
    area.cols = 44;
    area.value = JSON.stringify(value, null, 2);
    area.setAttribute("aria-label", "JSON input");
    form.append(area);
    form.__read = () => JSON.parse(area.value || "{}");
    return form;
  }

  for (const [key, property] of Object.entries(properties)) {
    const label = document.createElement("label");
    label.textContent = `${property.title ?? key}${required.has(key) ? " *" : ""}`;
    let input;
    if (Array.isArray(property.enum)) {
      input = document.createElement("select");
      for (const option of property.enum) {
        const element = document.createElement("option");
        element.value = String(option);
        element.textContent = String(option);
        input.append(element);
      }
    } else if (property.type === "boolean") {
      input = document.createElement("input");
      input.type = "checkbox";
    } else if (property.type === "number" || property.type === "integer") {
      input = document.createElement("input");
      input.type = "number";
    } else {
      input = document.createElement("input");
      input.type = "text";
    }
    input.name = key;
    if (value[key] !== undefined) {
      if (input.type === "checkbox") input.checked = Boolean(value[key]);
      else input.value = String(value[key]);
    }
    label.append(input);
    form.append(label);
  }

  form.__read = () => {
    const output = {};
    for (const [key, property] of Object.entries(properties)) {
      const input = form.elements.namedItem(key);
      if (!input) continue;
      if (input.type === "checkbox") output[key] = input.checked;
      else if (input.type === "number") {
        if (input.value !== "") output[key] = Number(input.value);
      } else if (input.value !== "") output[key] = input.value;
      else if (required.has(key)) output[key] = property.type === "boolean" ? false : "";
    }
    return output;
  };
  return form;
}
