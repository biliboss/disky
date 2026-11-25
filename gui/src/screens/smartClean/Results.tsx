import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import Check from "lucide-solid/icons/check";
import ChevronDown from "lucide-solid/icons/chevron-down";
import ShieldCheck from "lucide-solid/icons/shield-check";
import type { CategoryView, CleanupView } from "../../lib/contract";
import { NUMERIC_CLASS, formatBytes } from "../../lib/format";
import { iconByName } from "../../shell/iconByName";
import { listedCategories, type SmartCleanState } from "./phase";
import {
  EMPTY_RESULTS,
  NO_ITEMS,
  cleanLabel,
  disclosureLabel,
  resultsHeading,
  resultsKicker,
  safetyNote,
  safetyTag,
  selectedBytes,
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
  /** "Clean <total>". The confirmation dialog is REQ-DCLEAN-007's. */
  onClean?: () => void;
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
 * @implements REQ-DCLEAN-005
 * @implements REQ-DCLEAN-006
 * @implements REQ-DCLEAN-019
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
      <Show when={listed().length > 0} fallback={<EmptyResults onBack={props.onBack} />}>
        <header class="results-head">
          <div class="results-heading">
            <div class="results-kicker" data-testid="results-kicker">
              {resultsKicker(listed().length, props.state.openedBy)}
            </div>
            <h1 class="results-title">{resultsHeading(listed())}</h1>
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
  return (
    <li class="result" data-category-id={c().id} data-selected={String(props.selected)}>
      <div class="result-row">
        <button
          type="button"
          role="checkbox"
          class="result-check"
          aria-checked={props.selected}
          aria-label={c().name}
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
            <span class={tag().class} data-testid="safety-tag">
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
              <For each={c().items}>
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
        </Show>
      </div>
    </li>
  );
}

/** REQ-DCLEAN-019.A1: nothing reclaimable — a message and "Back to home", no Clean. */
function EmptyResults(props: { onBack: () => void }) {
  return (
    <div class="results-empty" data-testid="results-empty">
      <h1 class="results-title">{EMPTY_RESULTS}</h1>
      <div>
        <button type="button" class="btn btn-primary" onClick={() => props.onBack()}>
          Back to home
        </button>
      </div>
    </div>
  );
}
