import { For, Show, createSignal, onMount } from "solid-js";
import { useNavigate } from "@solidjs/router";
import Bot from "lucide-solid/icons/bot";
import { loadCleanupView, setPrefs } from "../lib/engine";
import { CHECK_FREQUENCIES, parseEngineError, type EngineError, type Suggestion } from "../lib/contract";
import { diskState, setFromView, setGoal } from "../lib/diskState";
import { NUMERIC_CLASS, formatBytes } from "../lib/format";
import { useDelayedBusy } from "../lib/busy";
import { GoalTag } from "../shell/GoalTag";
import { reviewFromNotification, reviewSuggestion } from "./smartClean/phase";
import { setSmartCleanState } from "./smartClean/store";
import {
  ASSISTANT_HEADING,
  AGENT_SUBTITLE,
  AGENT_TITLE,
  CHECKS_NOTE,
  CHECKS_TITLE,
  EMPTY_FEED,
  FREQUENCY_LABELS,
  GOAL_MAX_GB,
  GOAL_MIN_GB,
  GOAL_SLIDER_LABEL,
  GOAL_STEP_GB,
  GOAL_TITLE,
  KEEP_AT_LEAST,
  KEEP_FREE,
  PRIVACY_NOTE,
  actionLabels,
  agentStatus,
  goalFill,
  goalNote,
  goalToSlider,
  kindTagClass,
  sliderToGoal,
  statusLine,
  whenLabel,
} from "./assistant/assistantCopy";
import { assistantState, dismiss, reloadAssistant, restoreGoalCard, visibleSuggestions } from "./assistant/store";
import "./settings/settings.css";
import "./assistant/assistant.css";

/**
 * The Assistant screen (REQ-DSCR-009): the status line and heading, one
 * card per engine suggestion (REQ-DSCR-008), and the side panel
 * (REQ-DSCR-010). Layout follows snapshot.html "Assistant".
 *
 * A card's primary action only ever opens a review: "Review & confirm"
 * opens Smart Clean's results with the Safe categories selected under the
 * confirm dialog, "Review" with the card's categories selected and the
 * first expanded — both prepared by the selected agent, so the kicker reads
 * "Prepared by <agent>" and the history title is "Assistant cleanup"
 * (A3). Nothing is cleaned until "Clean now". "Not now" / "Dismiss" hide a
 * card for the session (A4).
 *
 * The side panel saves every change to the preferences store at once
 * (`set_prefs`) and then reloads the engine's view, so the goal tag, the
 * meter's goal marker and the feed follow without a restart (REQ-DSCR-010.A2).
 *
 * @implements REQ-DSCR-009
 * @implements REQ-DSCR-010
 */
export function Assistant() {
  const navigate = useNavigate();
  const [error, setError] = createSignal<EngineError | null>(null);
  const [draftGb, setDraftGb] = createSignal<number | null>(null);
  /** The card whose review is opening, and the error its review hit. */
  const [opening, setOpening] = createSignal<string | null>(null);
  const [reviewError, setReviewError] = createSignal<{ key: string; error: EngineError } | null>(null);
  /** The spinner shows only once opening crosses 200 ms (REQ-DCLEAN-017). */
  const openingBusy = useDelayedBusy(() => opening() !== null);
  onMount(() => void reloadAssistant());

  const view = () => assistantState().view;
  const disk = () => diskState()?.disk ?? view()?.disk;
  const goalBytes = () => {
    const d = draftGb();
    return d !== null ? sliderToGoal(d) : (view()?.goal_bytes ?? 0);
  };

  /** Open the review the card offers (A3): the engine's view, then Smart Clean. */
  async function review(s: Suggestion) {
    const v = view();
    if (!v || opening()) return;
    setError(null);
    setReviewError(null);
    setOpening(s.key);
    try {
      // Smart Clean acts only on what this view lists (REQ-DCLEAN-029.A1).
      const cleanup = await loadCleanupView();
      setFromView(cleanup);
      setSmartCleanState((st) =>
        s.key === "goal"
          ? reviewFromNotification(st, cleanup.categories, v.agent_name)
          : reviewSuggestion(st, cleanup.categories, s.categories, v.agent_name),
      );
      navigate("/");
    } catch (e) {
      // Shown on the card that was clicked: the feed's own alert can be
      // scrolled out of sight.
      setReviewError({ key: s.key, error: parseEngineError(e) });
    } finally {
      setOpening(null);
    }
  }

  /** Save one preference, then show the engine's view of the store. */
  async function save(key: string, value: string) {
    setError(null);
    try {
      const p = await setPrefs(key, value);
      if (key === "goal_bytes") {
        setGoal(p.goal_bytes);
        restoreGoalCard();
      }
      await reloadAssistant();
    } catch (e) {
      setError(parseEngineError(e));
    }
  }

  async function saveGoal(gb: number) {
    await save("goal_bytes", String(sliderToGoal(gb)));
    setDraftGb(null);
  }

  return (
    <section class="assistant" data-screen-label="Assistant" aria-labelledby="assistant-title" data-testid="assistant">
      <div class="assistant-feed">
        <header class="assistant-head">
          <div class="assistant-status" data-testid="assistant-status">
            {view() ? statusLine(view()!.agent_name, view()!.checks) : ""}
          </div>
          <h1 class="assistant-title" id="assistant-title">
            {ASSISTANT_HEADING}
          </h1>
        </header>
        <Show when={error() ?? assistantState().error}>
          {(e) => (
            <p class="assistant-error" role="alert">
              {e().detail}
            </p>
          )}
        </Show>
        <Show when={view()}>
          {(v) => (
            <>
              <For each={visibleSuggestions()}>
                {(s) => (
                  <SuggestionCard
                    suggestion={s}
                    agentName={v().agent_name}
                    opening={opening() === s.key}
                    busy={openingBusy() && opening() === s.key}
                    error={reviewError()?.key === s.key ? reviewError()!.error : null}
                    onReview={() => void review(s)}
                    onDismiss={() => dismiss(s.key)}
                  />
                )}
              </For>
              <Show when={visibleSuggestions().length === 0}>
                <p class="assistant-empty" data-testid="assistant-empty">
                  {EMPTY_FEED}
                </p>
              </Show>
            </>
          )}
        </Show>
      </div>

      <aside class="assistant-side" aria-label="Assistant settings" data-testid="assistant-side">
        <Show when={view() && disk()}>
          <div class="card assistant-panel assistant-goal">
            <div class="assistant-panel-head">
              <span class="card-title assistant-panel-title" id="goal-title">
                {GOAL_TITLE}
              </span>
              <GoalTag freeBytes={disk()!.free_bytes} goalBytes={goalBytes()} />
            </div>
            <div class="assistant-goal-line">
              <span>{KEEP_AT_LEAST}</span>
              <span class={`assistant-goal-value ${NUMERIC_CLASS}`} data-testid="assistant-goal">
                {formatBytes(goalBytes())}
              </span>
              <span>{KEEP_FREE}</span>
            </div>
            <input
              type="range"
              class="assistant-range"
              min={GOAL_MIN_GB}
              max={GOAL_MAX_GB}
              step={GOAL_STEP_GB}
              value={goalToSlider(goalBytes())}
              aria-label={GOAL_SLIDER_LABEL}
              aria-valuetext={formatBytes(goalBytes())}
              onInput={(e) => setDraftGb(Number(e.currentTarget.value))}
              onChange={(e) => void saveGoal(Number(e.currentTarget.value))}
            />
            <div class="assistant-fill" aria-hidden="true">
              <div class="assistant-fill-bar" style={{ width: goalFill(disk()!.free_bytes, goalBytes()) }} />
            </div>
            <div class="assistant-note" data-testid="assistant-goal-note">
              {goalNote(disk()!.free_bytes)}
            </div>
          </div>
        </Show>

        <Show when={view()}>
          {(v) => (
            <>
              <div class="card assistant-panel assistant-checks">
                <div class="assistant-panel-head">
                  <span class="card-title assistant-panel-title" id="checks-title">
                    {CHECKS_TITLE}
                  </span>
                  <button
                    type="button"
                    class="switch"
                    role="switch"
                    aria-checked={v().checks.enabled ? "true" : "false"}
                    aria-labelledby="checks-title"
                    aria-describedby="checks-note"
                    onClick={() => void save("checks.enabled", String(!v().checks.enabled))}
                  >
                    <span class="switch-knob" aria-hidden="true" />
                  </button>
                </div>
                <div class="seg assistant-seg" role="radiogroup" aria-labelledby="checks-title">
                  <For each={CHECK_FREQUENCIES}>
                    {(f) => (
                      <label class="seg-opt">
                        <input
                          type="radio"
                          name="check-frequency"
                          value={f}
                          checked={v().checks.frequency === f}
                          onChange={() => void save("checks.frequency", f)}
                        />
                        <span>{FREQUENCY_LABELS[f]}</span>
                      </label>
                    )}
                  </For>
                </div>
                <div class="assistant-note" id="checks-note">
                  {CHECKS_NOTE}
                </div>
              </div>

              <div class="card assistant-panel assistant-agents" role="group" aria-labelledby="agent-title">
                <span class="card-title" id="agent-title">
                  {AGENT_TITLE}
                </span>
                <span class="assistant-note">{AGENT_SUBTITLE}</span>
                <For each={v().agents}>
                  {(a) => {
                    const on = () => v().agent === a.id;
                    return (
                      <button
                        type="button"
                        class="agent-option"
                        aria-pressed={on() ? "true" : "false"}
                        disabled={!a.found}
                        data-testid={`agent-${a.id}`}
                        onClick={() => {
                          if (a.found && !on()) void save("agent", a.id);
                        }}
                      >
                        <span class="agent-dot" aria-hidden="true" />
                        <span class="agent-text">
                          <span class="agent-name">{a.name}</span>
                          <span class="agent-status">{agentStatus(a, on())}</span>
                        </span>
                      </button>
                    );
                  }}
                </For>
                <div class="assistant-privacy">{PRIVACY_NOTE}</div>
              </div>
            </>
          )}
        </Show>
      </aside>
    </section>
  );
}

interface SuggestionCardProps {
  suggestion: Suggestion;
  agentName: string;
  /** Its review is opening (the engine is loading the view). */
  opening: boolean;
  /** Opening has taken over 200 ms: show the spinner. */
  busy: boolean;
  /** What opening its review hit, if anything. */
  error: EngineError | null;
  onReview: () => void;
  onDismiss: () => void;
}

/**
 * One feed card (REQ-DSCR-009.A2): the agent icon and name, when, the kind
 * tag and the engine's text; when actionable, the primary and secondary
 * buttons and the size. The goal card sits on the accent-100 surface.
 */
function SuggestionCard(props: SuggestionCardProps) {
  const s = () => props.suggestion;
  const labels = () => actionLabels(s().key);
  return (
    <article
      class={s().key === "goal" ? "assistant-card assistant-card-goal" : "assistant-card"}
      data-testid={`suggestion-${s().key}`}
      aria-label={`${s().kind}: ${s().text}`}
    >
      <div class="assistant-card-head">
        <span class="assistant-card-icon" aria-hidden="true">
          <Bot class="assistant-card-glyph" strokeWidth={2.75} />
        </span>
        <span class="assistant-card-agent">{props.agentName}</span>
        <span class="assistant-card-when" data-testid="suggestion-when">
          {whenLabel(s().at)}
        </span>
        <span class={kindTagClass(s().key)}>{s().kind}</span>
      </div>
      <p class="assistant-card-text">{s().text}</p>
      <Show when={s().actionable}>
        <div class="assistant-card-actions">
          <button
            type="button"
            class="btn btn-primary"
            aria-busy={props.opening ? "true" : "false"}
            disabled={props.opening}
            onClick={() => props.onReview()}
          >
            <Show when={props.busy}>
              <span class="busy-spinner" aria-hidden="true" />
            </Show>
            {labels().primary}
          </button>
          <button type="button" class="btn btn-secondary" onClick={() => props.onDismiss()}>
            {labels().secondary}
          </button>
          <span class="assistant-card-spacer" />
          <span class={`assistant-card-size ${NUMERIC_CLASS}`} data-testid="suggestion-size">
            {s().bytes !== null ? formatBytes(s().bytes!) : ""}
          </span>
        </div>
      </Show>
      <Show when={props.error}>
        {(e) => (
          <p class="assistant-error" role="alert">
            {e().detail}
          </p>
        )}
      </Show>
    </article>
  );
}
