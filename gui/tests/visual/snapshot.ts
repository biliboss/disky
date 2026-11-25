import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import type { Page } from "@playwright/test";

/** Playwright runs with gui/ as cwd. */
const GUI = process.cwd();
export const SNAPSHOT_URL = pathToFileURL(resolve(GUI, "../specs/disk-cleanup/snapshot.html")).href;
export const BASELINE_DIR = resolve(GUI, "tests/visual/__baseline__");
const CDN_CACHE = resolve(BASELINE_DIR, "cdn");

/** Same mask colour on both sides so masked regions compare equal. */
export const MASK_COLOR = "#FF00FF";

/** Regions of the 1200 × 780 window the shell owns (REQ-DCLEAN-032.A1). */
export const REGIONS = {
  titlebar: { x: 0, y: 0, width: 1200, height: 40 },
  sidebar: { x: 0, y: 40, width: 236, height: 740 },
} as const;

/**
 * The @fontsource faces the app bundles, inlined as data: URIs. The snapshot's
 * Google Fonts request is answered with these, so both renders rasterise the
 * exact same font files and only real layout differences show up.
 */
function bundledFontCss(): string {
  const sheets = [
    "@fontsource/caprasimo/400.css",
    "@fontsource/figtree/400.css",
    "@fontsource/figtree/600.css",
    "@fontsource/figtree/700.css",
  ];
  return sheets
    .map((rel) => {
      const file = resolve(GUI, "node_modules", rel);
      const dir = resolve(file, "..");
      return readFileSync(file, "utf8").replace(/url\(\.\/files\/([^)]+)\)/g, (_m, name: string) => {
        const b64 = readFileSync(resolve(dir, "files", name)).toString("base64");
        const type = name.endsWith(".woff2") ? "woff2" : "woff";
        return `url(data:font/${type};base64,${b64})`;
      });
    })
    .join("\n");
}

/**
 * Render specs/disk-cleanup/snapshot.html as the visual reference.
 * Its runtime loads React/Babel from unpkg; those responses are cached under
 * __baseline__/cdn so later runs are offline. Allowed deviations applied:
 * brand text "Diskee" → "disky" (A2), and the window's rounded corners and
 * drop shadow are removed because macOS draws them around the real window.
 * With `startScreen`, the prototype opens on that screen: its `startScreen`
 * prop default is rewritten before the runtime boots (DOMContentLoaded), as
 * the design tool's prop editor would set it.
 * Returns the app window's top-left offset on the page.
 */
export async function openSnapshot(
  page: Page,
  opts: { startScreen?: "clean" | "agent" | "map" | "large" | "dupes" | "history" | "settings" } = {},
): Promise<{ x: number; y: number }> {
  mkdirSync(CDN_CACHE, { recursive: true });
  const fontCss = bundledFontCss();
  if (opts.startScreen) {
    await page.addInitScript((screen) => {
      document.addEventListener(
        "DOMContentLoaded",
        () => {
          const el = document.querySelector("script[data-dc-script]");
          const raw = el?.getAttribute("data-props");
          if (!el || !raw) return;
          const props = JSON.parse(raw);
          props.startScreen.default = screen;
          el.setAttribute("data-props", JSON.stringify(props));
        },
        { capture: true, once: true },
      );
    }, opts.startScreen);
  }

  // The HTML parser foster-parents a `<sc-for>` written directly inside
  // `<tbody>` out of the table, leaving its row template behind as one static
  // empty row (the Large files table). Put the template back inside its loop,
  // and the loop inside the tbody, before the prototype's runtime boots — the
  // markup as authored, which only the parser rearranged.
  await page.addInitScript(() => {
    document.addEventListener(
      "DOMContentLoaded",
      () => {
        for (const loop of document.querySelectorAll("sc-for")) {
          const table = loop.nextElementSibling;
          if (loop.children.length > 0 || table?.tagName !== "TABLE") continue;
          const body = table.querySelector("tbody");
          const row = body?.querySelector(":scope > tr");
          if (!body || !row) continue;
          loop.append(row);
          body.append(loop);
        }
      },
      { capture: true, once: true },
    );
  });

  await page.setViewportSize({ width: 1400, height: 900 });
  await page.route("https://fonts.googleapis.com/**", (route) =>
    route.fulfill({ contentType: "text/css", body: fontCss, headers: { "access-control-allow-origin": "*" } }),
  );
  await page.route("https://fonts.gstatic.com/**", (route) => route.abort());
  await page.route("https://unpkg.com/**", async (route) => {
    const url = route.request().url();
    const cached = resolve(CDN_CACHE, encodeURIComponent(url));
    const headers = { "access-control-allow-origin": "*", "content-type": "application/javascript" };
    if (existsSync(cached)) return route.fulfill({ body: readFileSync(cached), headers });
    const res = await route.fetch();
    const body = await res.body();
    if (res.ok()) writeFileSync(cached, body);
    return route.fulfill({ status: res.status(), body, headers });
  });

  await page.goto(SNAPSHOT_URL);
  const win = page.locator('[data-screen-label="App window"]');
  await win.waitFor();
  await page.waitForFunction(() => document.querySelectorAll("aside button").length === 7);

  await page.evaluate(() => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n.nodeValue?.includes("Diskee")) n.nodeValue = n.nodeValue.replaceAll("Diskee", "disky");
    }
    for (const el of document.querySelectorAll("[aria-label*=Diskee]")) {
      el.setAttribute("aria-label", el.getAttribute("aria-label")!.replaceAll("Diskee", "disky"));
    }
    const w = document.querySelector<HTMLElement>('[data-screen-label="App window"]')!;
    w.style.borderRadius = "0";
    w.style.boxShadow = "none";
  });
  await page.evaluate(() => document.fonts.ready);

  const box = (await win.boundingBox())!;
  return { x: box.x, y: box.y };
}

export function offset(region: { x: number; y: number; width: number; height: number }, by: { x: number; y: number }) {
  return { ...region, x: region.x + by.x, y: region.y + by.y };
}

export function baselinePath(name: string) {
  return resolve(BASELINE_DIR, name);
}
