import { createResource, createSignal, For, Show } from "solid-js";
import X from "lucide-solid/icons/x";
import { getPrefs, pickFolder, setPrefs } from "../lib/engine";
import { parseEngineError, SCAN_SCHEDULES, type EngineError, type Preferences } from "../lib/contract";
import { useDelayedBusy } from "../lib/busy";
import {
  ADD_FOLDER,
  EXCLUDED_DESC,
  EXCLUDED_TITLE,
  SCHEDULE_DESC,
  SCHEDULE_LABELS,
  SCHEDULE_TITLE,
  SETTINGS_TOGGLES,
  removeFolderLabel,
} from "./settings/settingsCopy";
import "./settings/settings.css";

/**
 * The Settings screen (REQ-DSCR-016): the four toggles, the scheduled-scan
 * period and the excluded folders. The engine's preferences store is the
 * only source (REQ-DSCR-001.A2): the screen shows what `get_prefs` returns,
 * every change is one `set_prefs` call made at once, and the screen then
 * shows the store as saved — never a local copy.
 *
 * @implements REQ-DSCR-016
 */
export function Settings() {
  const [loaded] = createResource(getPrefs);
  const [saved, setSaved] = createSignal<Preferences | null>(null);
  const [error, setError] = createSignal<EngineError | null>(null);
  const busy = useDelayedBusy(() => loaded.loading);

  const prefs = (): Preferences | undefined => saved() ?? (loaded.state === "ready" ? loaded() : undefined);
  const loadError = () => (loaded.state === "errored" ? parseEngineError(loaded.error) : null);

  async function save(key: string, value: string) {
    setError(null);
    try {
      setSaved(await setPrefs(key, value));
    } catch (e) {
      setError(parseEngineError(e));
    }
  }

  /** Save the excluded list as a JSON array (folder names may hold commas). */
  const saveExcluded = (folders: string[]) => save("excluded", JSON.stringify(folders));

  async function addFolder(current: Preferences) {
    setError(null);
    let folder: string | null;
    try {
      folder = await pickFolder();
    } catch (e) {
      setError(parseEngineError(e));
      return;
    }
    if (folder !== null && !current.excluded.includes(folder)) await saveExcluded([...current.excluded, folder]);
  }

  return (
    <section class="settings" data-screen-label="Settings" aria-labelledby="settings-title" data-testid="settings">
      <h1 class="settings-title" id="settings-title">
        Settings
      </h1>
      <Show when={loadError() ?? error()}>
        {(e) => (
          <p class="settings-error" role="alert">
            {e().detail}
          </p>
        )}
      </Show>
      <Show when={busy() && !prefs()}>
        <div class="busy" role="status" aria-busy="true" data-testid="busy">
          <span class="busy-spinner" aria-hidden="true" />
          Loading…
        </div>
      </Show>
      <Show when={prefs()}>
        {(p) => (
          <>
            <For each={SETTINGS_TOGGLES}>
              {(t) => {
                const on = () => p()[t.key];
                return (
                  <div class="settings-row">
                    <div class="settings-text">
                      <span class="settings-label" id={`pref-${t.key}-label`}>
                        {t.label}
                      </span>
                      <span class="settings-desc" id={`pref-${t.key}-desc`}>
                        {t.desc}
                      </span>
                    </div>
                    <button
                      type="button"
                      class="switch"
                      role="switch"
                      aria-checked={on() ? "true" : "false"}
                      aria-labelledby={`pref-${t.key}-label`}
                      aria-describedby={`pref-${t.key}-desc`}
                      onClick={() => save(t.key, String(!on()))}
                    >
                      <span class="switch-knob" aria-hidden="true" />
                    </button>
                  </div>
                );
              }}
            </For>
            <div class="settings-row">
              <div class="settings-text">
                <span class="settings-label" id="pref-schedule-label">
                  {SCHEDULE_TITLE}
                </span>
                <span class="settings-desc" id="pref-schedule-desc">
                  {SCHEDULE_DESC}
                </span>
              </div>
              <div
                class="seg settings-seg"
                role="radiogroup"
                aria-labelledby="pref-schedule-label"
                aria-describedby="pref-schedule-desc"
              >
                <For each={SCAN_SCHEDULES}>
                  {(s) => (
                    <label class="seg-opt">
                      <input
                        type="radio"
                        name="scheduled-scan"
                        value={s}
                        checked={p().scheduled_scan === s}
                        onChange={() => save("scheduled_scan", s)}
                      />
                      <span>{SCHEDULE_LABELS[s]}</span>
                    </label>
                  )}
                </For>
              </div>
            </div>
            <div class="settings-card">
              <div class="settings-card-head">
                <div class="settings-text">
                  <span class="settings-label" id="pref-excluded-label">
                    {EXCLUDED_TITLE}
                  </span>
                  <span class="settings-desc">{EXCLUDED_DESC}</span>
                </div>
                <button type="button" class="btn btn-secondary" onClick={() => addFolder(p())}>
                  {ADD_FOLDER}
                </button>
              </div>
              <ul class="settings-chips" aria-labelledby="pref-excluded-label">
                <For each={p().excluded}>
                  {(f) => (
                    <li class="tag tag-accent-2 settings-chip">
                      <span>{f}</span>
                      <button
                        type="button"
                        class="chip-remove"
                        aria-label={removeFolderLabel(f)}
                        onClick={() => saveExcluded(p().excluded.filter((x) => x !== f))}
                      >
                        <X class="chip-remove-glyph" aria-hidden="true" />
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            </div>
          </>
        )}
      </Show>
    </section>
  );
}
