import type { Component } from "solid-js";
import type { LucideProps } from "lucide-solid";
import AppWindow from "lucide-solid/icons/app-window";
import Box from "lucide-solid/icons/box";
import Clock from "lucide-solid/icons/clock";
import Code from "lucide-solid/icons/code";
import Download from "lucide-solid/icons/download";
import FileText from "lucide-solid/icons/file-text";
import Folder from "lucide-solid/icons/folder";
import Package from "lucide-solid/icons/package";
import Trash from "lucide-solid/icons/trash";

/**
 * Lucide icons the engine may name (`cleanup::Category::icon`), keyed by
 * Lucide icon name, never by category id (REQ-DCLEAN-005). A name not
 * listed here falls back to {@link FALLBACK_ICON}.
 */
const ICONS: Readonly<Record<string, Component<LucideProps>>> = {
  "app-window": AppWindow,
  box: Box,
  clock: Clock,
  code: Code,
  download: Download,
  "file-text": FileText,
  folder: Folder,
  package: Package,
  "trash": Trash, // lint-boundary-allow: Lucide icon name, not a category id
};

export const FALLBACK_ICON: Component<LucideProps> = Folder;

/** The Lucide component for an engine icon name. Unknown names get a folder. */
export function iconByName(name: string): Component<LucideProps> {
  return Object.hasOwn(ICONS, name) ? ICONS[name] : FALLBACK_ICON;
}

/** Whether `name` resolves to its own icon rather than the fallback. */
export function hasIcon(name: string): boolean {
  return Object.hasOwn(ICONS, name);
}
