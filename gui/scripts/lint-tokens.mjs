// Token lint (REQ-DCLEAN-031.A1): component code carries no literal colours
// or pixel sizes — those belong in the token sheets (src/styles/).
// Scans src/**/*.tsx, plus component stylesheets outside src/styles/.
// Usage: node scripts/lint-tokens.mjs   (exit 1 on any finding)
import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, relative } from "node:path";

// fileURLToPath, not .pathname: on Windows the pathname is "/D:/a/..." (REQ-XPLAT-010.A3).
const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SRC = join(ROOT, "src");
const TOKEN_SHEETS = join(SRC, "styles");

const RULES = [
  { name: "hex colour", re: /#[0-9a-fA-F]{3,8}\b/g },
  { name: "px literal", re: /\b\d+(?:\.\d+)?px\b/g },
  { name: "rgb/hsl literal", re: /\b(?:rgba?|hsla?|oklch)\(/g },
];

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "__tests__" || p === TOKEN_SHEETS) continue;
      yield* walk(p);
    } else if (p.endsWith(".tsx") || p.endsWith(".css")) {
      yield p;
    }
  }
}

/** Strip comments so documentation may cite source values. */
const stripComments = (src) =>
  src.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " ")).replace(/(^|[^:])\/\/.*$/gm, "$1");

const findings = [];
for (const file of walk(SRC)) {
  const lines = stripComments(readFileSync(file, "utf8")).split("\n");
  lines.forEach((line, i) => {
    for (const { name, re } of RULES) {
      for (const m of line.matchAll(re)) {
        findings.push(`${relative(ROOT, file)}:${i + 1}: ${name} "${m[0]}" — use a token`);
      }
    }
  });
}

if (findings.length) {
  console.error(findings.join("\n"));
  console.error(`\n${findings.length} literal(s) found. Add a token to src/styles/gui-tokens.css instead.`);
  process.exit(1);
}
console.log("lint:tokens — no literal colours or px sizes in component code.");
