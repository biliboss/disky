import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import ArrowLeft from "lucide-solid/icons/arrow-left";
import { loadMap, revealInFinder } from "../lib/engine";
import { parseEngineError, type EngineError, type MapChild } from "../lib/contract";
import { useDelayedBusy } from "../lib/busy";
import { formatBytes, NUMERIC_CLASS } from "../lib/format";
import { layout, tileColor } from "./spaceMap/treemap";
import {
  EMPTY_FOLDER,
  HINT,
  NO_SCANS,
  NO_SCANS_NOTE,
  OPEN_FOLDER,
  OTHER,
  REVEAL,
  SELECTED,
  SHARE,
  SIZE,
  START_SCAN,
  UP,
  crumbLine,
  mapTitle,
  otherDetail,
  shareOfDisk,
  tileLabel,
} from "./spaceMap/mapCopy";
import "./spaceMap/spaceMap.css";

/**
 * The Space map screen (REQ-DSCR-012): a treemap of one folder's immediate
 * children, from the engine's `load_map` (`disky map`). The engine supplies
 * every size and the volume capacity; the screen lays the sizes out with the
 * snapshot's split algorithm and lets the user select and drill in.
 *
 * Keyboard (A5): each tile is a button named "<name>, <size>"; Enter or
 * Space selects it, and Enter on the selected tile opens it.
 *
 * @implements REQ-DSCR-012
 */
/** A block thinner than this (percent of the treemap side) takes the small radius. */
const THIN_PERCENT = 8;

/** A block: an engine child, or the engine's "Other" (`other` = its child count). */
type Tile = MapChild & { weight: number; other: number | null };

export function SpaceMap() {
  const navigate = useNavigate();
  /** Folders opened below the root, outermost first; empty at the root. */
  const [trail, setTrail] = createSignal<string[]>([]);
  const [pick, setPick] = createSignal(0);
  const [error, setError] = createSignal<EngineError | null>(null);
  const [view] = createResource(
    () => ({ path: trail().at(-1) ?? null }),
    (s) => loadMap(s.path),
  );
  /** The last loaded map; never throws (an errored load reads as none). */
  const latest = () => (view.state === "errored" ? undefined : view.latest);
  const busy = useDelayedBusy(() => view.loading);
  const loadError = () => (view.state === "errored" ? parseEngineError(view.error) : null);

  /**
   * Tiles: children with bytes, laid out in the engine's order (largest
   * first), then the engine's "Other" block when it folded small children
   * away (REQ-DSCR-012.A2). Other is neither openable nor revealable.
   */
  const tiles = createMemo(() => {
    const v = latest();
    if (!v) return [];
    const kids: Tile[] = v.records.filter((c) => c.bytes > 0).map((c) => ({ ...c, weight: c.bytes, other: null }));
    if (v.other && v.other.bytes > 0) {
      kids.push({
        name: OTHER,
        path: v.path,
        bytes: v.other.bytes,
        has_children: false,
        weight: v.other.bytes,
        other: v.other.count,
      });
    }
    return layout(kids);
  });
  const selected = (): Tile | undefined => tiles()[Math.min(pick(), tiles().length - 1)];

  function open(c: MapChild | undefined) {
    if (!c?.has_children) return;
    setError(null);
    setPick(0);
    setTrail([...trail(), c.path]);
  }

  function up() {
    setError(null);
    setPick(0);
    setTrail(trail().slice(0, -1));
  }

  async function reveal(path: string) {
    setError(null);
    try {
      await revealInFinder(path);
    } catch (e) {
      setError(parseEngineError(e));
    }
  }

  function onTileKey(e: KeyboardEvent, i: number, c: MapChild) {
    if (e.key === "Enter" && i === pick()) {
      e.preventDefault();
      open(c);
    }
  }

  const noSnapshot = () => view.state === "ready" && view() === null;

  return (
    <Show
      when={!noSnapshot()}
      fallback={
        <section class="screen" data-screen-label="Space map" aria-labelledby="map-title" data-testid="space-map">
          <h1 class="screen-title" id="map-title">
            {NO_SCANS}
          </h1>
          <p class="screen-note">{NO_SCANS_NOTE}</p>
          <div>
            <button type="button" class="btn btn-primary" onClick={() => navigate("/")}>
              {START_SCAN}
            </button>
          </div>
        </section>
      }
    >
      <section class="map" data-screen-label="Space map" aria-labelledby="map-title" data-testid="space-map">
        <div class="map-head">
          <Show when={trail().length > 0}>
            <button type="button" class="btn btn-icon btn-secondary" aria-label={UP} onClick={up}>
              <ArrowLeft class="map-up-glyph" aria-hidden="true" />
            </button>
          </Show>
          <div class="map-heading">
            <Show when={latest()}>{(v) => <div class="map-crumb">{crumbLine(v())}</div>}</Show>
            <h1 class="map-title" id="map-title">
              <Show when={latest()} fallback="Space map">
                {(v) => <>{mapTitle(v())}</>}
              </Show>
            </h1>
          </div>
        </div>
        <div class="map-tiles" role="group" aria-label="Treemap" aria-busy={view.loading ? "true" : "false"}>
          <Show when={busy()}>
            <div class="busy" role="status" aria-busy="true" data-testid="busy">
              <span class="busy-spinner" aria-hidden="true" />
              Loading…
            </div>
          </Show>
          <Show when={latest() && tiles().length === 0 && !view.loading}>
            <p class="map-empty">{EMPTY_FOLDER}</p>
          </Show>
          <For each={tiles()}>
            {(t, i) => (
              <button
                type="button"
                class="map-tile"
                aria-label={t.other == null ? tileLabel(t.name, t.bytes) : tileLabel(`${t.name}, ${otherDetail(t.other)}`, t.bytes)}
                classList={{ "map-tile-other": t.other != null, "map-tile-thin": Math.min(t.w, t.h) < THIN_PERCENT }}
                aria-pressed={i() === pick() ? "true" : "false"}
                style={{
                  // Clamped to the slot's far edge: a slot thinner than the gutter
                  // collapses to nothing inside itself, never past the treemap.
                  left: `min(calc(${t.x}% + var(--map-tile-inset)), ${t.x + t.w}%)`,
                  top: `min(calc(${t.y}% + var(--map-tile-inset)), ${t.y + t.h}%)`,
                  width: `calc(${t.w}% - var(--map-tile-gutter))`,
                  height: `calc(${t.h}% - var(--map-tile-gutter))`,
                  "background-color": tileColor(i()),
                }}
                onClick={() => setPick(i())}
                onDblClick={() => open(t)}
                onKeyDown={(e) => onTileKey(e, i(), t)}
              >
                {/* The padding lives on this inner box, never on the button: a
                    padded button can't shrink below its padding, so a thin block
                    would outgrow its slot. The button is exactly its slot. */}
                <span class="map-tile-body">
                  <span class="map-tile-name">{t.name}</span>
                  {/* A space between the spans so the visible text ("Users 212 GB")
                      sits inside the accessible name; flex layout never renders it. */}
                  {" "}
                  <span class={`map-tile-size ${NUMERIC_CLASS}`}>{formatBytes(t.bytes)}</span>
                </span>
              </button>
            )}
          </For>
        </div>
        <div class="map-side">
          <Show when={loadError() ?? error()}>
            {(e) => (
              <p class="map-error" role="alert">
                {e().detail}
              </p>
            )}
          </Show>
          <Show when={selected()}>
            {(s) => (
              <div class="card map-card" data-testid="map-selected">
                <span class="card-kicker map-card-kicker">{SELECTED}</span>
                <span class="card-title map-card-title">{s().name}</span>
                <span class="map-card-path">{s().other == null ? s().path : otherDetail(s().other!)}</span>
                <div class="map-card-row map-card-row-first">
                  <span>{SIZE}</span>
                  <span class={`map-card-value ${NUMERIC_CLASS}`}>{formatBytes(s().bytes)}</span>
                </div>
                <div class="map-card-row">
                  <span>{SHARE}</span>
                  <span class={`map-card-value ${NUMERIC_CLASS}`}>
                    {shareOfDisk(s().bytes, latest()?.capacity_bytes ?? 0)}
                  </span>
                </div>
                <div class="map-card-actions">
                  <Show when={s().has_children}>
                    <button type="button" class="btn btn-primary" onClick={() => open(s())}>
                      {OPEN_FOLDER}
                    </button>
                  </Show>
                  <Show when={s().other == null}>
                    <button type="button" class="btn btn-secondary" onClick={() => reveal(s().path)}>
                      {REVEAL}
                    </button>
                  </Show>
                </div>
              </div>
            )}
          </Show>
          <span class="map-hint">{HINT}</span>
        </div>
      </section>
    </Show>
  );
}
