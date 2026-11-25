import type { Component } from "solid-js";
import type { LucideProps } from "lucide-solid";
import Sparkle from "lucide-solid/icons/sparkle";
import Bot from "lucide-solid/icons/bot";
import File from "lucide-solid/icons/file";
import Copy from "lucide-solid/icons/copy";
import History from "lucide-solid/icons/history";
import SlidersHorizontal from "lucide-solid/icons/sliders-horizontal";
import { SpaceMapIcon } from "./icons";

export type NavId =
  | "clean"
  | "assistant"
  | "map"
  | "large"
  | "dupes"
  | "history"
  | "settings";

export interface NavItemDef {
  id: NavId;
  label: string;
  icon: Component<LucideProps>;
  route: string;
  /** Carries the pending-suggestions badge (count supplied by the shell). */
  badge?: true;
}

/**
 * The seven sidebar entries, in snapshot order (snapshot.html `navItems`).
 * Icons are Lucide (lucide-solid 0.527.0 — the last release whose geometry
 * matches the snapshot's paths); Space Map uses the snapshot's own geometry.
 */
export const NAV_ITEMS: readonly NavItemDef[] = [
  { id: "clean", label: "Smart Clean", icon: Sparkle, route: "/" },
  { id: "assistant", label: "Assistant", icon: Bot, route: "/assistant", badge: true },
  { id: "map", label: "Space Map", icon: SpaceMapIcon, route: "/space-map" },
  { id: "large", label: "Large files", icon: File, route: "/large-files" },
  { id: "dupes", label: "Duplicates", icon: Copy, route: "/duplicates" },
  { id: "history", label: "History", icon: History, route: "/history" },
  { id: "settings", label: "Settings", icon: SlidersHorizontal, route: "/settings" },
];
