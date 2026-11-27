// Engine-boundary lint (REQ-DCLEAN-001.A3, REQ-DCLEAN-027.A4).
//
// The GUI is a client of the disky engine. gui/src/** and gui/src-tauri/src/**
// must hold no file walking, size summing, or remove/rename code of their own,
// and no hard-coded cleanup category ids: categories come from the engine
// table (src/cleanup.rs CATEGORIES), whose ids this script reads.
//
// Allowed: Tauri `invoke` only from src/lib/engine.ts, and only for the
// command names in COMMANDS (each delegates to `disky::*` in the backend).
// A line may opt out with a trailing `lint-boundary-allow: <reason>` comment;
// the reason is mandatory.
//
// Usage: node scripts/lint-boundary.mjs   (exit 1 on any finding)
import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, relative } from "node:path";

// fileURLToPath, not .pathname: on Windows the pathname is "/D:/a/..." (REQ-XPLAT-010.A3).
const ROOT = fileURLToPath(new URL("..", import.meta.url));
const REPO = join(ROOT, "..");
const SRC = join(ROOT, "src");
const TAURI_SRC = join(ROOT, "src-tauri", "src");
const ENGINE_TS = join(SRC, "lib", "engine.ts");

/** Tauri commands the frontend may call; each one delegates to the engine. */
const COMMANDS = [
  "load_cleanup_view",
  "scan",
  "stop_scan",
  "clean",
  "restore",
  "open_full_disk_access",
  "get_prefs",
  "set_prefs",
  "pick_folder",
  "list_history",
  "load_map",
  "reveal_in_finder",
  "load_large",
  "trash_large",
  "load_dupes",
  "trash_dupes",
  "load_assistant",
];

/** Category ids, read from the engine's single table. */
function categoryIds() {
  const rs = readFileSync(join(REPO, "src", "cleanup.rs"), "utf8");
  const table = rs.slice(rs.indexOf("pub const CATEGORIES"), rs.indexOf("];", rs.indexOf("pub const CATEGORIES")));
  const ids = [...table.matchAll(/^\s{8}name:\s*"([^"]+)"/gm)].map((m) => m[1]);
  const trash = rs.match(/pub const TRASH_CATEGORY: &str = "([^"]+)"/);
  if (trash && table.includes("name: TRASH_CATEGORY")) ids.push(trash[1]);
  if (ids.length < 10) throw new Error(`lint-boundary: read only ${ids.length} category ids from src/cleanup.rs`);
  return ids;
}

const TS_RULES = [
  { name: "filesystem module import", re: /from\s+["'](?:node:)?(?:fs|fs\/promises|child_process)["']/ },
  { name: "Tauri fs/shell plugin", re: /@tauri-apps\/plugin-(?:fs|shell)/ },
  { name: "remove/rename call", re: /\b(?:unlink|rmdir|rm|rename|remove|removeFile|removeDir)(?:Sync)?\s*\(/ },
  { name: "directory walk", re: /\b(?:readdir|readDir|opendir|walk)(?:Sync)?\s*\(/ },
  { name: "size summing", re: /\breduce\s*\(.*\bbytes\b|\bbytes\s*\+=|\+=\s*\w+\.bytes\b/ },
];

const RS_RULES = [
  { name: "std::fs use", re: /\bstd::fs\b|\buse\s+std::\{[^}]*\bfs\b/ },
  { name: "remove/rename call", re: /\b(?:remove_dir_all|remove_file|remove_dir|rename|trash::delete)\s*\(/ },
  { name: "directory walk", re: /\b(?:read_dir|WalkDir|jwalk|walkdir)\b/ },
  { name: "size summing", re: /\.sum\s*(?:::<[^>]*>)?\s*\(|\bbytes\s*\+=/ },
  { name: "metadata read", re: /\b(?:symlink_metadata|metadata)\s*\(/ },
];

function* walk(dir, exts) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "__tests__") continue;
      yield* walk(p, exts);
    } else if (exts.some((e) => p.endsWith(e)) && !/\.test\.tsx?$/.test(p)) {
      yield p;
    }
  }
}

/** Blank out comments (keeping line numbers) so docs may name the rules. */
const stripComments = (src) =>
  src.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " ")).replace(/(^|[^:])\/\/.*$/gm, "$1");

const ALLOW = /lint-boundary-allow:\s*\S/;

const ids = categoryIds();
const idLiteral = new RegExp(`(["'\`])(${ids.map((i) => i.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})\\1`, "g");

const findings = [];
function check(file, rules) {
  const raw = readFileSync(file, "utf8").split("\n");
  const code = stripComments(raw.join("\n")).split("\n");
  code.forEach((line, i) => {
    if (ALLOW.test(raw[i])) return;
    const at = `${relative(ROOT, file)}:${i + 1}`;
    for (const { name, re } of rules) {
      if (re.test(line)) findings.push(`${at}: ${name} — the engine does this (REQ-DCLEAN-001.A3)`);
    }
    for (const m of line.matchAll(idLiteral)) {
      findings.push(`${at}: category id ${m[0]} — categories come from the engine (REQ-DCLEAN-027.A4)`);
    }
    if (file.endsWith(".ts") || file.endsWith(".tsx")) {
      for (const m of line.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*(["'`])([^"'`]*)\1/g)) {
        if (file !== ENGINE_TS) findings.push(`${at}: invoke outside src/lib/engine.ts`);
        else if (!COMMANDS.includes(m[2])) findings.push(`${at}: command "${m[2]}" is not an allow-listed engine delegation`);
      }
    }
  });
}

for (const f of walk(SRC, [".ts", ".tsx"])) check(f, TS_RULES);
for (const f of walk(TAURI_SRC, [".rs"])) check(f, RS_RULES);

if (findings.length) {
  console.error(findings.join("\n"));
  console.error(`\n${findings.length} boundary finding(s). Move the logic into the disky engine and call it through a Tauri command.`);
  process.exit(1);
}
console.log(`lint:boundary — gui/ delegates to the engine; ${ids.length} category ids checked.`);
