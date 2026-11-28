import { expect, test } from "@playwright/test";

// The responsive and contrast budgets of GEMBA-WALK-SPEC §8, measured rather than asserted from
// the design: the canvas is an image and not a focus trap, the palette is a keyboard path, a
// 390 px viewport and a 200 % zoom keep the page free of horizontal scroll, the controls meet the
// 44 px target, and ink on paper meets WCAG 2.2 AA.

async function settle(page: any) {
  await expect(page.locator("#world-canvas")).toHaveAttribute("data-engine", /ecs|fallback/, {
    timeout: 20_000,
  });
}

test("the canvas is an image and is not in the tab order", async ({ page }) => {
  await page.goto("/");
  await settle(page);
  const canvas = page.locator("#world-canvas");
  await expect(canvas).toHaveAttribute("role", "img");
  await expect(canvas).toHaveAttribute("aria-label", /.+/);
  expect(await canvas.evaluate((node: HTMLElement) => node.tabIndex)).toBeLessThan(0);
});

test("the palette is reachable by keyboard and closes on Escape", async ({ page }) => {
  await page.goto("/");
  await settle(page);
  const palette = page.getByRole("button", { name: "Palette" });
  await palette.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Palette" });
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((node: HTMLElement) => node.contains(document.activeElement))).toBe(
    true,
  );
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await palette.evaluate((node: HTMLElement) => node === document.activeElement)).toBe(true);
});

test("at 390 px there is no horizontal scroll and the controls meet 44 px", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Boards", level: 1 })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow, "horizontal overflow in px").toBeLessThanOrEqual(1);

  // The palette is a control surface too: open it so its rows are measured with the rest.
  await page.getByRole("button", { name: "Palette" }).click();
  await expect(page.getByRole("dialog", { name: "Palette" })).toBeVisible();

  const controls = await page.evaluate(() =>
    [...document.querySelectorAll("button, a.button, .palette-row")]
      .filter((node) => (node as HTMLElement).offsetParent !== null)
      .map((node) => ({
        label: (node.textContent ?? "").trim().slice(0, 24),
        height: Math.round((node as HTMLElement).getBoundingClientRect().height),
      })),
  );
  test.info().annotations.push({
    type: "a11y.controls",
    description: JSON.stringify(controls),
  });
  expect(controls.length).toBeGreaterThan(0);
  const under = controls.filter((control) => control.height < 44);
  expect(under, `controls under 44 px: ${JSON.stringify(under)}`).toEqual([]);
});

test("at 200 % zoom there is no horizontal scroll", async ({ page }) => {
  await page.setViewportSize({ width: 780, height: 900 });
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Boards", level: 1 })).toBeVisible();
  const overflow = await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
    return document.documentElement.scrollWidth - window.innerWidth;
  });
  expect(overflow, "horizontal overflow at 200 % zoom, in px").toBeLessThanOrEqual(1);
  await page.evaluate(() => {
    document.documentElement.style.zoom = "";
  });
});

test("ink on paper meets WCAG 2.2 AA", async ({ page }) => {
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Boards", level: 1 })).toBeVisible();
  const ratios = await page.evaluate(() => {
    function parse(value: string): number[] {
      const match = value.match(/rgba?\(([^)]+)\)/);
      if (!match) return [0, 0, 0, 1];
      const parts = match[1]!.split(",").map((part) => Number.parseFloat(part.trim()));
      return [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0, parts[3] ?? 1];
    }
    function channel(value: number): number {
      const c = value / 255;
      return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
    }
    function luminance(rgb: number[]): number {
      return 0.2126 * channel(rgb[0]!) + 0.7152 * channel(rgb[1]!) + 0.0722 * channel(rgb[2]!);
    }
    function background(node: HTMLElement | null): number[] {
      let current = node;
      while (current) {
        const value = parse(getComputedStyle(current).backgroundColor);
        if (value[3] > 0) return value;
        current = current.parentElement;
      }
      return [255, 255, 255, 1];
    }
    function ratio(node: HTMLElement | null): number {
      if (!node) return 0;
      const foreground = parse(getComputedStyle(node).color);
      const back = background(node);
      const a = luminance(foreground);
      const b = luminance(back);
      const light = Math.max(a, b);
      const dark = Math.min(a, b);
      return (light + 0.05) / (dark + 0.05);
    }
    const heading = document.querySelector("#screen h1") as HTMLElement | null;
    const body = document.querySelector("#screen p") as HTMLElement | null;
    const button = document.querySelector(".destination-actions .button") as HTMLElement | null;
    return { heading: ratio(heading), body: ratio(body), button: ratio(button) };
  });
  test.info().annotations.push({
    type: "a11y.contrast",
    description: JSON.stringify(ratios),
  });
  // 4.5:1 for body text; 3:1 for large text (the h1) and for a control's label at its size.
  expect(ratios.body).toBeGreaterThanOrEqual(4.5);
  expect(ratios.heading).toBeGreaterThanOrEqual(3);
  expect(ratios.button).toBeGreaterThanOrEqual(4.5);
});
