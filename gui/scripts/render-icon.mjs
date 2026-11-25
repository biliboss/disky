// Renders the brand mark (src/shell/Brand.tsx geometry, Organic token values)
// to a 1024×1024 PNG for `tauri icon`. Usage:
//   node scripts/render-icon.mjs src-tauri/icons/source.png && npx tauri icon src-tauri/icons/source.png
import { chromium } from "@playwright/test";
import { readFileSync } from "node:fs";

const out = process.argv[2] ?? "src-tauri/icons/source.png";
const tokens = readFileSync(new URL("../src/styles/tokens.css", import.meta.url), "utf8");

const html = `<!doctype html><style>${tokens} html,body{margin:0;background:transparent}</style>
<svg width="1024" height="1024" viewBox="0 0 34 34">
  <rect x="0" y="0" width="34" height="34" rx="11" fill="var(--color-text)"/>
  <circle cx="17" cy="17" r="9" fill="none" stroke="var(--color-accent)" stroke-dasharray="42.4 56.5" transform="rotate(-90 17 17)" stroke-width="5" stroke-linecap="round"/>
  <circle cx="17" cy="17" r="9" fill="none" stroke="var(--color-accent-2-300)" stroke-dasharray="4 56.5" stroke-dashoffset="-48.5" transform="rotate(-90 17 17)" stroke-width="5" stroke-linecap="round"/>
  <circle cx="17" cy="17" r="2.6" fill="var(--color-bg)"/>
</svg>`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 } });
await page.setContent(html);
await page.locator("svg").screenshot({ path: out, omitBackground: true });
await browser.close();
console.log(`wrote ${out}`);
