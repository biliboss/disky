import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import Check from "lucide-solid/icons/check";
import ChevronDown from "lucide-solid/icons/chevron-down";
import ShieldCheck from "lucide-solid/icons/shield-check";
import type { CategoryView, CleanupView } from "../../lib/contract";
import { NUMERIC_CLASS, formatBytes } from "../../lib/format";
import { iconByName } from "../../shell/iconByName";
import { FULL_DISK_ACCESS_LINK, SKIPPED_HINT, skippedPaths } from "./errorCopy";
import { hasFullDiskAccess, OS } from "../../lib/os";
import { listedCategories, type SmartCleanState } from "./phase";
import {
  EMPTY_RESULTS,
  NO_ITEMS,
  cleanLabel,
  disclosureLabel,
  moreItemsLabel,
  resultsHeading,
  resultsKicker,
  safetyNote,
  safetyTag,
  selectedBytes,
  visibleItems,
} from "./resultsCopy";

export interface ResultsProps {
  view: CleanupView;
  state: SmartCleanState;
  onToggleExpand: (id: string) => void;
  onToggleSelect: (id: string) => void;
  onSelectSafe: () => void;
  onSelectAll: () => void;
  /** "Back to home" / "Cancel": return to idle, nothing cleaned. */
  onBack: () => void;
  /** "Clean <total>": opens the confirm dialog (REQ-DCLEAN-007); never cleans by itself. */
  onClean?: () => void;
  /** Why the last clean reclaimed nothing (REQ-DCLEAN-018.A4), if it did. */
  cleanError?: string | null;
  /** Opens System Settings at Full Disk Access (REQ-DCLEAN-018.A2). */
  onOpenFullDiskAccess?: () => void;
}

/**
 * Results phase (REQ-DCLEAN-005): kicker, "Found <total> you can clear",
 * "Safe only" / "Select all" (REQ-DCLEAN-006.A2, no "Select none", A6), one
 * row per listed engine category (A1) with its selection control, icon,
 * name, safety tag (A2), description, size and disclosure (A3, A4), and the
 * footer note and "Clean <selected total>" (REQ-DCLEAN-006.A4, A5). Empty
 * categories are not listed, and with none left the empty state replaces
 * the list and the Clean action (REQ-DCLEAN-019). Layout follows
 * snapshot.html "Smart clean · review".
 *
 * Errors (REQ-DCLEAN-018): a clean that reclaimed nothing returns here with
 * the selection kept and says so (A4); a scan that skipped unreadable paths
 * says how many, with a link to Full Disk Access (A2).
 *
 * @implements REQ-DCLEAN-005
 * @implements REQ-DCLEAN-006
 * Controls (REQ-DCLEAN-012): the buttons use the design-system classes; the
 * selection control is the themed round `.result-check`. Each checkbox is
 * named by its category and described by its safety tag (REQ-DCLEAN-021.A1,
 * A5). An expanded category lists at most its 200 largest items, then
 * "+N more" (REQ-DCLEAN-030.A3).
 *
 * @implements REQ-DCLEAN-018
 * @implements REQ-DCLEAN-019
 * @implements REQ-DCLEAN-012
 * @implements REQ-DCLEAN-021
 * @implements REQ-DCLEAN-030
 */
export function Results(props: ResultsProps) {
  const listed = () => listedCategories(props.view.categories);
  const total = () => selectedBytes(listed(), props.state.selected);
  return (
    <div class="results" data-testid="results" data-screen-label="Smart clean · review">
      <Show when={props.view.is_stale}>
        {/* REQ-DCLEAN-029.A2: warn before cleaning a snapshot older than 24 h. */}
        <p class="results-stale" role="status" data-testid="stale-warning">
          <span class="tag tag-accent">Out of date</span> This scan is more than a day old. Scan again before
          cleaning.
        </p>
      </Show>
      <Show when={props.cleanError}>
        <div class="error-state results-error" role="alert" data-testid="clean-error">
          {props.cleanError}
        </div>
      </Show>
      <Show when={props.view.skipped_paths > 0}>
        <p class="notice skipped-note" role="status" data-testid="skipped-paths">
          {skippedPaths(props.view.skipped_paths)}. {SKIPPED_HINT}{" "}
          <Show when={hasFullDiskAccess(OS)}>
            <button type="button" class="notice-link" onClick={() => props.onOpenFullDiskAccess?.()}>
              {FULL_DISK_ACCESS_LINK}
            </button>
          </Show>
        </p>
      </Show>
      <Show when={listed().length > 0} fallback={<EmptyResults onBack={props.onBack} />}>
        <header class="results-head">
          <div class="results-heading">
            <div class="results-kicker" data-testid="results-kicker">
              {resultsKicker(listed().length, props.state.openedBy)}
            </div>
            {/* tabindex -1: focus lands here on entering results (REQ-DCLEAN-020.A4) and
                when the confirm dialog has no opener to return to (REQ-DCLEAN-022.A4). */}
            <h1 class="results-title" tabIndex={-1} data-phase-heading data-testid="results-heading">
              {resultsHeading(listed())}
            </h1>
          </div>
          <button type="button" class="btn btn-ghost" onClick={() => props.onSelectSafe()}>
            Safe only
          </button>
          <button type="button" class="btn btn-ghost" onClick={() => props.onSelectAll()}>
            Select all
          </button>
        </header>
        <ul class="results-list" role="list" data-testid="categories">
          <For each={listed()}>
            {(c) => (
              <CategoryRow
                category={c}
                selected={props.state.selected.includes(c.id)}
                expanded={props.state.expanded === c.id}
                onToggleSelect={() => props.onToggleSelect(c.id)}
                onToggleExpand={() => props.onToggleExpand(c.id)}
              />
            )}
          </For>
        </ul>
        <footer class="results-foot">
          <ShieldCheck class="results-foot-glyph" aria-hidden="true" />
          <span class="results-foot-note" data-testid="safety-note">
            {safetyNote(props.view.preferences.trash_by_default)}
          </span>
          <button type="button" class="btn btn-secondary" onClick={() => props.onBack()}>
            Cancel
          </button>
          <button
            type="button"
            class="btn btn-primary results-clean"
            data-testid="clean"
            disabled={total() === 0}
            onClick={() => props.onClean?.()}
          >
            {cleanLabel(total())}
          </button>
        </footer>
      </Show>
    </div>
  );
}

interface CategoryRowProps {
  category: CategoryView;
  selected: boolean;
  expanded: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
}

/** One category card: header row, and its items when expanded. */
function CategoryRow(props: CategoryRowProps) {
  const c = () => props.category;
  const tag = () => safetyTag(c().safety);
  const itemsId = () => `items-${c().id}`;
  const tagId = () => `safety-${c().id}`;
  const items = () => visibleItems(c().items);
  return (
    <li class="result" data-category-id={c().id} data-selected={String(props.selected)}>
      <div class="result-row">
        <button
          type="button"
          role="checkbox"
          class="result-check"
          aria-checked={props.selected}
          aria-label={c().name}
          aria-describedby={tagId()}
          data-checked={String(props.selected)}
          onClick={() => props.onToggleSelect()}
        >
          <Show when={props.selected}>
            <Check class="result-check-glyph" aria-hidden="true" />
          </Show>
        </button>
        <span class="result-icon" data-icon={c().icon}>
          <Dynamic component={iconByName(c().icon)} class="result-glyph" aria-hidden="true" />
        </span>
        <div class="result-text">
          <div class="result-title">
            <span class="result-name">{c().name}</span>
            <span id={tagId()} class={tag().class} data-testid="safety-tag">
              {tag().label}
            </span>
          </div>
          <span class="result-desc">{c().description}</span>
        </div>
        <span class={`result-size ${NUMERIC_CLASS}`} data-testid="category-size">
          {formatBytes(c().bytes)}
        </span>
        <button
          type="button"
          class="btn btn-icon btn-ghost result-disclose"
          aria-expanded={props.expanded}
          aria-controls={itemsId()}
          aria-label={disclosureLabel(c().name)}
          data-expanded={String(props.expanded)}
          onClick={() => props.onToggleExpand()}
        >
          <ChevronDown class="chevron" aria-hidden="true" />
        </button>
      </div>
      <div class="result-items" id={itemsId()} hidden={!props.expanded} data-testid="category-items">
        <Show when={c().items.length > 0} fallback={<p class="result-no-items">{NO_ITEMS}</p>}>
          <table class="result-items-table">
            <tbody>
              <For each={items().shown}>
                {(it) => (
                  <tr class="result-item">
                    <td class="item-name">{it.name}</td>
                    {/* A5: ellipsised in the cell; full path as tooltip and accessible name. */}
                    <td class="item-path" title={it.path} aria-label={it.path}>
                      {it.path}
                    </td>
                    <td class={`item-size ${NUMERIC_CLASS}`}>{formatBytes(it.bytes)}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          <Show when={items().more > 0}>
            <p class="result-more" data-testid="more-items">
              {moreItemsLabel(items().more)}
            </p>
          </Show>
        </Show>
      </div>
    </li>
  );
}

/** REQ-DCLEAN-019.A1: nothing reclaimable — a message and "Back to home", no Clean. */
function EmptyResults(props: { onBack: () => void }) {
  return (
    <div class="results-empty" data-testid="results-empty">
      <h1 class="results-title" tabIndex={-1} data-phase-heading>
        {EMPTY_RESULTS}
      </h1>
      <div>
        <button type="button" class="btn btn-primary" onClick={() => props.onBack()}>
          Back to home
        </button>
      </div>
    </div>
  );
}
