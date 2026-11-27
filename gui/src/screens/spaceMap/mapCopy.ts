/**
 * Space map copy (REQ-DSCR-012), from the snapshot's Space map markup. Sizes
 * and the share of disk come from engine bytes (`disky map`); this module
 * only words and formats them.
 */
import type { MapView } from "../../lib/contract";
import { formatBytes } from "../../lib/format";
import { OS, revealLabel, rootLabel } from "../../lib/os";

/** The volume's display name when the scan root is the filesystem root. */
export const ROOT_LABEL = rootLabel(OS);
export const HINT = "Click a block to inspect it, double-click to open.";
export const SELECTED = "Selected";
export const SIZE = "Size";
export const SHARE = "Share of disk";
export const OPEN_FOLDER = "Open folder";
/** "Reveal in Finder", "Show in Explorer" or "Show in folder" (REQ-XPLAT-008). */
export const REVEAL = revealLabel(OS);
export const UP = "Up";
export const NO_SCANS = "No scans yet";
export const NO_SCANS_NOTE = "Run a scan to see what's taking up room.";
export const START_SCAN = "Start scan";
/** A folder with nothing measurable inside (gap-fill). */
export const EMPTY_FOLDER = "Nothing to show in this folder.";

function basename(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

/** Crumb parts: the scan root ("Macintosh HD" for `/`), then each folder below it. */
export function crumbs(view: Pick<MapView, "path" | "root">): string[] {
  const rootLabel = view.root === "/" ? ROOT_LABEL : basename(view.root);
  const below = view.path === view.root ? "" : view.path.slice(view.root === "/" ? 1 : view.root.length + 1);
  return [rootLabel, ...below.split("/").filter(Boolean)];
}

/** "Macintosh HD  /  Users  /  me" — the snapshot joins with two spaces each side. */
export function crumbLine(view: Pick<MapView, "path" | "root">): string {
  return crumbs(view).join("  /  ");
}

/** "<folder> · <size>". */
export function mapTitle(view: Pick<MapView, "path" | "root" | "total_bytes">): string {
  const c = crumbs(view);
  return `${c[c.length - 1]} · ${formatBytes(view.total_bytes)}`;
}

/** The block the engine sums small children into (`MapView.other`). */
export const OTHER = "Other";

/** "12 smaller items" — the Other block's detail line. */
export function otherDetail(count: number): string {
  return `${count} smaller ${count === 1 ? "item" : "items"}`;
}

/** A tile's accessible name: "<name>, <size>" (REQ-DSCR-012.A5). */
export function tileLabel(name: string, bytes: number): string {
  return `${name}, ${formatBytes(bytes)}`;
}

/** Share of the volume to one decimal: "41.4%". */
export function shareOfDisk(bytes: number, capacityBytes: number): string {
  if (capacityBytes <= 0) return "—";
  return `${((bytes / capacityBytes) * 100).toFixed(1)}%`;
}
