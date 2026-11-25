/**
 * The snapshot prototype's eight review categories (specs/disk-cleanup/
 * snapshot.html, `CATS`) as engine `CategoryView`s. Sizes are the
 * prototype's GB figures in exact decimal bytes, so every category's bytes
 * equal the sum of its items (REQ-DCLEAN-027.A3). Ids are fixture ids, not
 * engine ids. Icons are the Lucide names the engine table gives the
 * matching categories.
 */
const GB = 1e9;

type Row = [name: string, desc: string, icon: string, safe: boolean, items: [string, string, number][]];

const ROWS: Row[] = [
  ["System caches", "Rebuilt automatically when needed", "box", true, [
    ["Font caches", "/Library/Caches/com.apple.ATS", 1.1],
    ["Spotlight temp", "/private/var/folders/…/mds", 1.6],
    ["Update leftovers", "/Library/Updates", 1.5],
  ]],
  ["App caches", "Chrome, Spotify, Slack, VS Code", "app-window", true, [
    ["Chrome", "~/Library/Caches/Google/Chrome", 1.2],
    ["Spotify", "~/Library/Caches/com.spotify.client", 0.9],
    ["Slack", "~/Library/Application Support/Slack/Cache", 0.6],
    ["VS Code", "~/Library/Application Support/Code/Cache", 0.4],
  ]],
  ["Temporary files", "Left behind by installers and apps", "clock", true, [
    ["Installer temp", "/private/tmp", 1.2],
    ["App temp", "~/Library/Containers/…/tmp", 0.6],
  ]],
  ["Logs & crash reports", "Older than 7 days", "file-text", true, [
    ["System logs", "/private/var/log", 0.4],
    ["Diagnostic reports", "~/Library/Logs/DiagnosticReports", 0.2],
  ]],
  ["Trash", "1,284 items already in the Trash", "trash", true, [["Trash", "~/.Trash", 2.4]]],
  ["App leftovers", "Support files from 6 apps you uninstalled", "package", false, [
    ["Sketch", "~/Library/Application Support/com.bohemiancoding.sketch3", 1.4],
    ["Zoom", "~/Library/Application Support/zoom.us", 0.8],
    ["Others (4)", "~/Library/Preferences, ~/Library/Containers", 0.5],
  ]],
  ["Old downloads", "Not opened in 6 months", "download", false, [
    ["Xcode_15.dmg", "~/Downloads", 3.2],
    ["Figma-installer.zip", "~/Downloads", 0.4],
    ["tax-scans-2024/", "~/Downloads", 1.7],
  ]],
  ["Developer junk", "node_modules, build caches, old simulators", "code", false, [
    ["node_modules (4 idle projects)", "~/Projects/**/node_modules", 4.1],
    ["Xcode DerivedData", "~/Library/Developer/Xcode/DerivedData", 2.6],
    ["Cargo target dirs", "~/Projects/**/target", 1.2],
  ]],
];

export function snapshotReviewCategories() {
  return ROWS.map(([name, description, icon, safe, rows], i) => {
    const items = rows.map(([n, path, gb]) => ({ name: n, path, bytes: Math.round(gb * GB) }));
    return {
      id: `visual_cat_${i}`,
      name,
      description,
      icon,
      safety: safe ? ("safe" as const) : ("review" as const),
      bytes: items.reduce((acc, it) => acc + it.bytes, 0),
      items,
    };
  });
}
