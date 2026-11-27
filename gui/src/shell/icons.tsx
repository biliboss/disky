import type { LucideProps } from "lucide-solid";
import { Icon } from "lucide-solid";

/**
 * Space Map icon. The snapshot's path is four 8×8 rounded squares, which no
 * Lucide release ships (layout-grid uses 7×7, rx 1), so the snapshot geometry
 * is rendered through Lucide's own Icon base for identical stroke handling.
 */
const SPACE_MAP_NODE: [string, Record<string, string>][] = [
  ["rect", { x: "3", y: "3", width: "8", height: "8", rx: "2", key: "tl" }],
  ["rect", { x: "13", y: "3", width: "8", height: "8", rx: "2", key: "tr" }],
  ["rect", { x: "13", y: "13", width: "8", height: "8", rx: "2", key: "br" }],
  ["rect", { x: "3", y: "13", width: "8", height: "8", rx: "2", key: "bl" }],
];

export function SpaceMapIcon(props: LucideProps) {
  return <Icon {...props} iconNode={SPACE_MAP_NODE as never} name="space-map" />;
}
