/**
 * The OS the app runs on, for the few words that differ per OS
 * (REQ-XPLAT-008, REQ-XPLAT-010.A2). Read from the webview's user agent:
 * WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux. Everything
 * else (screens, flows, numbers) is the same on every OS.
 */
export type Os = "macos" | "windows" | "linux";

export function detectOs(userAgent: string): Os {
  if (/Windows/i.test(userAgent)) return "windows";
  if (/Mac OS X|Macintosh/i.test(userAgent)) return "macos";
  if (/Linux|X11|CrOS/i.test(userAgent)) return "linux";
  return "macos";
}

/** This webview's OS. */
export const OS: Os = detectOs(typeof navigator === "undefined" ? "" : navigator.userAgent);

/** The Space map's reveal action: "Reveal in Finder" / "Show in Explorer" / "Show in folder". */
export function revealLabel(os: Os): string {
  return os === "windows" ? "Show in Explorer" : os === "linux" ? "Show in folder" : "Reveal in Finder";
}

/** The name the map gives the `/` root: "Macintosh HD" on macOS, "File System" on Linux. */
export function rootLabel(os: Os): string {
  return os === "linux" ? "File System" : "Macintosh HD";
}

/** Only macOS gates reads behind Full Disk Access. */
export function hasFullDiskAccess(os: Os): boolean {
  return os === "macos";
}

/** Why a scan could not read some paths, worded for the OS. */
export function skippedHint(os: Os): string {
  return hasFullDiskAccess(os)
    ? "disky needs Full Disk Access to read them."
    : "disky does not have permission to read them.";
}
