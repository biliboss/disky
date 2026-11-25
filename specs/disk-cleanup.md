---
id: DISK-CLEANUP-DOC
title: Disk Cleanup
owner: Selvakumar Esra
priority: high
source: specs/disk-cleanup/source.md
snapshot: specs/disk-cleanup/snapshot.html
tokens: specs/disk-cleanup/tokens.css
version: 1
---

<!-- id: DISK-CLEANUP-DOC -->
# Disk Cleanup

The Disk Cleanup component ("Smart Clean") guides a user from a disk-usage overview through a scan, a categorised review, an explicit confirmation, the cleaning run, and a completion summary with undo. It is imported from a Claude Design handoff bundle. The bundle has no design decision record, so behaviour is taken from the prototype logic in the snapshot. Where the snapshot is silent, this spec fills the gap with a stated default.

In scope: the five Smart Clean phases (idle, scanning, results, cleaning, done), the confirmation dialog, the shared disk-usage meter, and the below-goal notification shown on the idle phase.

Out of scope (separate specs): Assistant, Space Map, Large files, Duplicates, History, and Settings screens. This spec only names their entry points as navigation targets and consumes two Settings values (trash-by-default, safe-only preselection) and the space-goal value as inputs.

Visual values (colour, type, spacing, radius, elevation, motion) live in `tokens.css`. This spec names tokens and never their values.

Resolved decisions (2026-11-24, recorded in AGENTS.md):

- Surface: a Tauri + SolidJS desktop app in `gui/`, reinstated under the AGENTS.md "Amendment (2026-11-24) — desktop GUI reinstated" (earn-back criterion 2).
- Design system: Organic, ratified for `gui/` by the AGENTS.md Design Feel exception. Its `--shadow-*` elevation tokens are allowed. The snapshot is the pixel reference.
- Product name: the brand text reads "disky". The snapshot's "Diskee" is the one deliberate visual deviation.
- Categories: the design's categories and their safe/review classification come from an extension of disky's cleanup engine (REQ-DCLEAN-033), and the CLI exposes them too.
- Restore: a clean-history log (REQ-DCLEAN-034) backs Undo and History restore.
- Scope: the app shell plus the Smart Clean flow. The Assistant, Space Map, Large files, Duplicates, History and Settings screens get their own specs. Until then their sidebar entries open a placeholder (REQ-DCLEAN-032), and the Settings values this spec reads use their defaults: trash-by-default on, safe-only preselection on, notifications on, assistant on.

<!-- id: REQ-DCLEAN-001 -->
## The GUI MUST be a desktop client of the disky engine and MUST NOT fork its logic

The component ships as a Tauri + SolidJS desktop app in `gui/`. Scanning, querying, cleanup, history and restore MUST be done by the disky engine, either the `disky` library crate called from the Tauri backend or the CLI's `--format json` envelopes. The GUI MUST NOT reimplement any of them. Every mutation goes through disky's cleanup contract: dry-run by default, apply only on explicit confirmation, and reversible by default.

**Acceptance**
<!-- id: REQ-DCLEAN-001.A1 -->
- Every operation the GUI performs (scan, list categories, clean, undo, restore) has an equivalent `disky` CLI command that produces the same result on the same snapshot.
<!-- id: REQ-DCLEAN-001.A2 -->
- No code path removes or moves a file without passing through the confirmation contract in REQ-DCLEAN-007.
<!-- id: REQ-DCLEAN-001.A3 -->
- `gui/` contains no file-walking, size-summing or file-deletion code of its own. Code review rejects a change that adds any.

<!-- id: REQ-DCLEAN-002 -->
## The disk-usage meter MUST report physical free space against the space goal

A persistent meter shows the volume name, free space, a used-share bar, a marker at the goal position, and the text "Goal: keep <goal> free". The idle and done phases also show a ring for used share, with the free amount and the total capacity as its label. A goal tag reads "Below your goal" when free space is below the goal, and "Goal met" otherwise.

**Acceptance**
<!-- id: REQ-DCLEAN-002.A1 -->
- Free and used figures are physical bytes on the device, consistent with `df` for that volume, not the logical sum of file sizes.
<!-- id: REQ-DCLEAN-002.A2 -->
- With free bytes strictly less than goal bytes, the goal tag reads "Below your goal" and uses the `tag-accent` treatment; otherwise it reads "Goal met" and uses `tag-accent-2`.
<!-- id: REQ-DCLEAN-002.A3 -->
- The goal marker sits at the bar position equal to (capacity − goal) / capacity.
<!-- id: REQ-DCLEAN-002.A4 -->
- After a successful clean, meter, ring and tag reflect the new free space without a manual refresh.
<!-- id: REQ-DCLEAN-002.A5 -->
- Used fill MUST use `--color-accent` on idle and `--color-accent-2` on done; the track MUST use `--color-surface` for the ring and `--color-neutral-200` for the sidebar bar.

<!-- id: REQ-DCLEAN-003 -->
## The idle phase MUST offer a scan as the primary action

The idle phase shows: a last-scan kicker ("Last scan <relative time>"), the heading "Let's clear some room.", the description promising that nothing is removed until reviewed and that everything goes to the Trash first, a primary "Start scan" action, a secondary "Explore disk" action (navigates to Space Map), and three shortcut cards (Assistant, Space Map, Large files).

**Acceptance**
<!-- id: REQ-DCLEAN-003.A1 -->
- Activating "Start scan" enters the scanning phase with progress at 0, collapses any expanded category, and dismisses the goal notification.
<!-- id: REQ-DCLEAN-003.A2 -->
- The last-scan kicker is derived from the newest snapshot's timestamp; with no prior snapshot it reads "No scans yet".
<!-- id: REQ-DCLEAN-003.A3 -->
- The Assistant shortcut description reads "<n> suggestions waiting" when n > 0, else "Watching your space goal".
<!-- id: REQ-DCLEAN-003.A4 -->
- The Large files shortcut count comes from data, not a fixed string. The snapshot's "8 files over 2 GB" is fixture copy.
<!-- id: REQ-DCLEAN-003.A5 -->
- The description text MUST stay truthful to the configured mode. When trash-by-default is off, it MUST NOT claim "everything goes to the Trash first", and instead reads "Nothing is removed until you review it."

<!-- id: REQ-DCLEAN-004 -->
## The scanning phase MUST show live progress and MUST be stoppable

The scanning phase shows the heading "Looking around…", the path currently being examined (truncated with an ellipsis on overflow), a percentage, a progress bar, and one row per category. A completed row shows a check and its size; the category in progress shows "Checking…"; pending rows are blank. A secondary "Stop" action is offered.

**Acceptance**
<!-- id: REQ-DCLEAN-004.A1 -->
- The percentage is monotonically non-decreasing and reaches 100 only when the scan has finished.
<!-- id: REQ-DCLEAN-004.A2 -->
- On completion, the component enters the results phase automatically.
<!-- id: REQ-DCLEAN-004.A3 -->
- Activating "Stop" ends the scan and returns to idle; no results are shown and no file is modified. The partial snapshot disky keeps on cancel is not offered for review.
<!-- id: REQ-DCLEAN-004.A4 -->
- Category row sizes appear only once that category is complete; no size is shown speculatively.
<!-- id: REQ-DCLEAN-004.A5 -->
- Progress fill MUST use `--color-accent` over a `--color-surface` track.

<!-- id: REQ-DCLEAN-005 -->
## The results phase MUST list every reclaimable category with its safety class

The heading reads "Found <total> you can clear", where the total is the sum of all listed categories. The kicker reads "Scan finished · <n> categories", or "Prepared by <agent> · review before cleaning" when the assistant opened the review. Each category row shows a selection control, an icon, the name, a safety tag ("Safe" or "Review"), a one-line description, its size, and a disclosure control. Expanding a row lists its items as name, path and size.

**Acceptance**
<!-- id: REQ-DCLEAN-005.A1 -->
- Every listed category and item comes from the scan result; no category is shown that the scan did not produce.
<!-- id: REQ-DCLEAN-005.A2 -->
- A "Safe" category uses the `tag-accent-2` treatment and a "Review" category uses `tag-accent`.
<!-- id: REQ-DCLEAN-005.A3 -->
- At most one category is expanded at a time; expanding another collapses the first, and re-activating the open row collapses it.
<!-- id: REQ-DCLEAN-005.A4 -->
- The disclosure control rotates to indicate the expanded state, using `--motion-chevron`.
<!-- id: REQ-DCLEAN-005.A5 -->
- Item paths longer than their cell are truncated with an ellipsis, and the full path is available to assistive tech and as a native tooltip on hover.
<!-- id: REQ-DCLEAN-005.A6 -->
- The sum of item sizes in a category equals the category size, give or take the rounding of the size formatter.

<!-- id: REQ-DCLEAN-006 -->
## Category selection MUST default to the safe-only preference and MUST be user-adjustable

**Acceptance**
<!-- id: REQ-DCLEAN-006.A1 -->
- When the "preselect only safe categories" preference is on (default on, prop `safeMode`), a new scan preselects exactly the "Safe" categories; when off, it preselects all categories.
<!-- id: REQ-DCLEAN-006.A2 -->
- "Safe only" selects exactly the Safe categories, and "Select all" selects every category.
<!-- id: REQ-DCLEAN-006.A3 -->
- Toggling a category's selection control flips only that category.
<!-- id: REQ-DCLEAN-006.A4 -->
- The footer primary action reads "Clean <selected total>", and the total updates on every selection change.
<!-- id: REQ-DCLEAN-006.A5 -->
- The footer safety note reads "Items move to the Trash and can be restored from History for 30 days." when trash-by-default is on, and "Trash is off in Settings, so items will be deleted permanently." when off.
<!-- id: REQ-DCLEAN-006.A6 -->
- No "Select none" action is offered, matching the snapshot; the user unticks categories one by one.

<!-- id: REQ-DCLEAN-007 -->
## Cleaning MUST NOT start without explicit confirmation in a modal dialog

Activating "Clean <total>" opens a modal titled "Clean <total>?". It lists each selected category with its size, a two-option mode switch ("Move to Trash" / "Delete permanently"), a note for the chosen mode, and "Cancel" / "Clean now" actions.

**Acceptance**
<!-- id: REQ-DCLEAN-007.A1 -->
- No file is moved or removed before "Clean now" is activated.
<!-- id: REQ-DCLEAN-007.A2 -->
- The mode initially matches the trash-by-default preference.
<!-- id: REQ-DCLEAN-007.A3 -->
- The note reads "You can restore everything from History for 30 days." for Trash, and "This can't be undone." for permanent delete.
<!-- id: REQ-DCLEAN-007.A4 -->
- "Cancel", Escape, and activating the backdrop close the dialog with the selection unchanged.
<!-- id: REQ-DCLEAN-007.A5 -->
- The dialog lists exactly the selected categories, and its title total equals the footer total.
<!-- id: REQ-DCLEAN-007.A6 -->
- When "Delete permanently" is chosen, the primary action uses the destructive treatment: `--color-danger` fill, an alias of `--color-accent-800` added to the GUI token sheet.
<!-- id: REQ-DCLEAN-007.A7 -->
- The backdrop MUST use `--overlay-backdrop`.

<!-- id: REQ-DCLEAN-008 -->
## The cleaning phase MUST report progress per category

The cleaning phase shows the heading "Tidying up…", a step label "Moving <category>…", a percentage, and a progress bar.

**Acceptance**
<!-- id: REQ-DCLEAN-008.A1 -->
- The step label names the category currently being processed, in selection order.
<!-- id: REQ-DCLEAN-008.A2 -->
- Progress fill MUST use `--color-accent-2` to distinguish cleaning from scanning.
<!-- id: REQ-DCLEAN-008.A3 -->
- The step verb matches the mode: "Moving" for Trash, "Deleting" for permanent. The snapshot always says "Moving"; this is a deliberate deviation.
<!-- id: REQ-DCLEAN-008.A4 -->
- No cancel is offered during cleaning, matching the snapshot.

<!-- id: REQ-DCLEAN-009 -->
## The done phase MUST report bytes actually freed and MUST offer undo only when reversible

The done phase shows a "Done" tag, the heading "<freed> cleared", a mode-dependent note, the actions "Back to home" (primary), "Undo" (secondary) and "View history" (ghost), and the ring with "now free" beneath the free amount.

**Acceptance**
<!-- id: REQ-DCLEAN-009.A1 -->
- "<freed>" equals the physical bytes actually reclaimed, not the pre-clean estimate. With a partial failure the two differ, and the reclaimed figure is shown.
<!-- id: REQ-DCLEAN-009.A2 -->
- After a Trash clean, the note reads "Everything went to the Trash. Changed your mind? Undo now or restore any time in the next 30 days."; after a permanent clean, it reads "Those files were deleted permanently."
<!-- id: REQ-DCLEAN-009.A3 -->
- "Undo" MUST NOT be offered after a permanent delete. This deviates from the snapshot, which shows Undo unconditionally.
<!-- id: REQ-DCLEAN-009.A4 -->
- "Undo" restores every trashed item to its original path, marks the history entry "Restored", updates the meter, and returns to idle.
<!-- id: REQ-DCLEAN-009.A5 -->
- A history entry is recorded with the title "Smart Clean" (or "Assistant cleanup" if the assistant opened the review), a timestamp, the cleaned category names, the freed bytes, and whether the delete was permanent.
<!-- id: REQ-DCLEAN-009.A6 -->
- "Back to home" returns to idle, and "View history" navigates to History.
<!-- id: REQ-DCLEAN-009.A7 -->
- Every "restore" or "30 days" claim in the copy is backed by the clean-history log (REQ-DCLEAN-034). If an item has already left the macOS Trash, restoring it reports it as "no longer in Trash" instead of failing silently.

<!-- id: REQ-DCLEAN-010 -->
## A below-goal notification MUST prompt a review and MUST NOT clean on its own

On the idle phase, a notification with "status" semantics appears when all of these hold: notifications are enabled (prop `showNotification`), the assistant is on, free space is below the goal, and the user has not dismissed it this session. Its text reads "You're down to <free> free, under your <goal> goal. I can clear <safe total> of safe junk once you confirm.", with the actions "Later" and "Review cleanup".

**Acceptance**
<!-- id: REQ-DCLEAN-010.A1 -->
- The notification is shown only on the idle phase, and only when every listed condition holds.
<!-- id: REQ-DCLEAN-010.A2 -->
- "Later" hides it for the rest of the session; it MAY reappear after an app restart.
<!-- id: REQ-DCLEAN-010.A3 -->
- "Review cleanup" opens the results phase with exactly the Safe categories selected and the confirmation dialog open. No file is touched until "Clean now".
<!-- id: REQ-DCLEAN-010.A4 -->
- "<safe total>" is computed from the latest scan's Safe categories. With no scan available, the notification is not shown.
<!-- id: REQ-DCLEAN-010.A5 -->
- The notification MUST NOT take keyboard focus when it appears.

<!-- id: REQ-DCLEAN-011 -->
## Navigation MUST be blocked during scanning and cleaning, and blocked entries MUST look disabled

**Acceptance**
<!-- id: REQ-DCLEAN-011.A1 -->
- While scanning or cleaning, activating any navigation entry does not change the screen.
<!-- id: REQ-DCLEAN-011.A2 -->
- While blocked, navigation entries are exposed as disabled to assistive tech and use the disabled treatment (REQ-DCLEAN-016). The snapshot ignores clicks silently; this is a gap-fill.
<!-- id: REQ-DCLEAN-011.A3 -->
- The current screen's entry is marked as current (accent fill per `--color-accent`, text in `--color-bg`) and exposed as the current page.
<!-- id: REQ-DCLEAN-011.A4 -->
- Leaving the done phase via navigation resets Smart Clean to idle.

<!-- id: REQ-DCLEAN-012 -->
## Every interactive control MUST render a themed default state from design-system classes

**Acceptance**
<!-- id: REQ-DCLEAN-012.A1 -->
- Buttons use the design-system button classes (primary, secondary, ghost, icon); no control relies on browser-default styling.
<!-- id: REQ-DCLEAN-012.A2 -->
- Selection controls render unchecked with a `--color-neutral-400` border and checked with a `--color-accent` fill and a `--color-bg` check, sized by `--checkbox-size`.
<!-- id: REQ-DCLEAN-012.A3 -->
- Category rows and cards use `--color-surface` on `--color-bg`, with radius from `--radius-md` / `--radius-card`.

<!-- id: REQ-DCLEAN-013 -->
## Interactive controls MUST show a hover state

**Acceptance**
<!-- id: REQ-DCLEAN-013.A1 -->
- The primary button hover uses `--color-accent-600`.
<!-- id: REQ-DCLEAN-013.A2 -->
- The secondary button hover uses `--overlay-hover`, and the ghost button hover uses `--overlay-ghost-hover`.
<!-- id: REQ-DCLEAN-013.A3 -->
- Shortcut cards on hover use `--color-neutral-200` as background.
<!-- id: REQ-DCLEAN-013.A4 -->
- Category rows SHOULD show a row hover using `--overlay-row-hover`.
<!-- id: REQ-DCLEAN-013.A5 -->
- Hover MUST NOT be the only way to reveal information (see REQ-DCLEAN-005.A5).

<!-- id: REQ-DCLEAN-014 -->
## Interactive controls MUST show a visible keyboard-focus state

**Acceptance**
<!-- id: REQ-DCLEAN-014.A1 -->
- Every focusable control shows the design-system focus-visible ring in `--color-accent` when focused via keyboard, and no ring when focused by pointer.
<!-- id: REQ-DCLEAN-014.A2 -->
- Segmented-control options show the ring on the option wrapper when their hidden radio has keyboard focus.
<!-- id: REQ-DCLEAN-014.A3 -->
- No control shows the browser-default focus ring.

<!-- id: REQ-DCLEAN-015 -->
## Interactive controls MUST show a pressed state

**Acceptance**
<!-- id: REQ-DCLEAN-015.A1 -->
- The primary button pressed state uses `--color-accent-700`.
<!-- id: REQ-DCLEAN-015.A2 -->
- The secondary button pressed state uses `--overlay-active`, and the ghost button uses `--overlay-ghost-active`.
<!-- id: REQ-DCLEAN-015.A3 -->
- The selected segmented option uses `--color-accent` fill with `--color-bg` text.

<!-- id: REQ-DCLEAN-016 -->
## Unavailable actions MUST render disabled and MUST NOT activate

**Acceptance**
<!-- id: REQ-DCLEAN-016.A1 -->
- "Clean <total>" is disabled when the selected total is zero.
<!-- id: REQ-DCLEAN-016.A2 -->
- Disabled controls use the design-system disabled treatment (reduced opacity, not-allowed cursor) and ignore pointer and keyboard activation. The treatment is the design system's built-in 45% opacity.
<!-- id: REQ-DCLEAN-016.A3 -->
- Disabled controls are exposed as disabled to assistive tech, and remain discoverable rather than being removed from the reading order.

<!-- id: REQ-DCLEAN-017 -->
## Asynchronous work MUST show a loading state only when it would otherwise feel stalled

**Acceptance**
<!-- id: REQ-DCLEAN-017.A1 -->
- Scanning and cleaning always show their phase screens (REQ-DCLEAN-004, REQ-DCLEAN-008) as the loading state.
<!-- id: REQ-DCLEAN-017.A2 -->
- Short waits (results loading, undo, restore) show a busy indicator only after the project's spinner threshold (AGENTS.md § Design Feel) has elapsed.
<!-- id: REQ-DCLEAN-017.A3 -->
- The pause between scan completion and the results phase MUST NOT exceed the spinner threshold (200 ms). The prototype's fixed pause is not reproduced.

<!-- id: REQ-DCLEAN-018 -->
## Failures MUST surface as an error state that names the cause and a recovery action

The snapshot shows no error state. This REQ is gap-filled from disky's error taxonomy.

**Acceptance**
<!-- id: REQ-DCLEAN-018.A1 -->
- A scan that cannot start because the snapshot is locked by another process shows an error naming that cause, with a "Try again" action. The copy reads "Another disky process is using the snapshot. Try again in a moment."
<!-- id: REQ-DCLEAN-018.A2 -->
- A scan that cannot read some paths for permission reasons completes, shows results for the readable paths, and states how many paths were skipped, with a link that opens the Full Disk Access pane in System Settings.
<!-- id: REQ-DCLEAN-018.A3 -->
- A clean where some paths fail leaves the failed paths untouched. The done phase lists the failed paths and reports only the bytes actually reclaimed.
<!-- id: REQ-DCLEAN-018.A4 -->
- A clean where every path fails does not show the done phase. It shows an error state and returns to results with the selection preserved.
<!-- id: REQ-DCLEAN-018.A5 -->
- Error states MUST use `--color-danger` (see REQ-DCLEAN-007.A6).

<!-- id: REQ-DCLEAN-019 -->
## A scan with nothing reclaimable MUST show an empty state instead of an empty list

**Acceptance**
<!-- id: REQ-DCLEAN-019.A1 -->
- When the scan yields zero categories, the results phase shows an empty-state message with "Back to home" and no "Clean" action. The copy reads "Nothing worth clearing right now."
<!-- id: REQ-DCLEAN-019.A2 -->
- A category whose size is zero is omitted from the list.
<!-- id: REQ-DCLEAN-019.A3 -->
- An expanded category with no items shows the one-line note "No items".

<!-- id: REQ-DCLEAN-020 -->
## The component MUST be fully operable by keyboard in a predictable focus order

**Acceptance**
<!-- id: REQ-DCLEAN-020.A1 -->
- Tab order follows visual order: navigation, then the phase heading region's actions, then the list rows (selection control, then disclosure), then the footer actions.
<!-- id: REQ-DCLEAN-020.A2 -->
- Every action reachable by pointer is reachable and activatable by Enter/Space.
<!-- id: REQ-DCLEAN-020.A3 -->
- Segmented controls move between options with the arrow keys.
<!-- id: REQ-DCLEAN-020.A4 -->
- On each phase change, focus moves to the new phase's heading.

<!-- id: REQ-DCLEAN-021 -->
## Controls MUST expose correct roles, names and states to assistive tech

**Acceptance**
<!-- id: REQ-DCLEAN-021.A1 -->
- Category selection controls are exposed as checkboxes with checked state, named by their category. This replaces the snapshot's generic "Select" label.
<!-- id: REQ-DCLEAN-021.A2 -->
- Disclosure controls expose expanded/collapsed state, reference the region they control, and are named "Show items in <category>".
<!-- id: REQ-DCLEAN-021.A3 -->
- Scan and clean progress bars are exposed as progress indicators with current value, minimum and maximum.
<!-- id: REQ-DCLEAN-021.A4 -->
- The usage ring and the sidebar bar expose a text equivalent ("<free> free of <capacity>"); decorative icons are hidden from assistive tech.
<!-- id: REQ-DCLEAN-021.A5 -->
- The safety tag is part of each row's accessible name or description, so the class is not conveyed by colour alone.

<!-- id: REQ-DCLEAN-022 -->
## The confirmation dialog MUST manage focus as a modal

**Acceptance**
<!-- id: REQ-DCLEAN-022.A1 -->
- The dialog is exposed as modal and labelled by its title.
<!-- id: REQ-DCLEAN-022.A2 -->
- On open, focus moves into the dialog on "Cancel", the non-destructive action.
<!-- id: REQ-DCLEAN-022.A3 -->
- Focus is trapped inside the dialog while it is open.
<!-- id: REQ-DCLEAN-022.A4 -->
- On close, focus returns to the control that opened it, or, when the dialog was opened from the notification, to the results heading.

<!-- id: REQ-DCLEAN-023 -->
## Status changes MUST be announced to screen readers without stealing focus

**Acceptance**
<!-- id: REQ-DCLEAN-023.A1 -->
- Scan and clean progress is announced politely at coarse intervals (for example, per category completed), not on every tick.
<!-- id: REQ-DCLEAN-023.A2 -->
- Phase completion ("Found <total> you can clear", "<freed> cleared") is announced.
<!-- id: REQ-DCLEAN-023.A3 -->
- The below-goal notification is announced via polite status semantics.

<!-- id: REQ-DCLEAN-024 -->
## Text and interface elements MUST meet WCAG 2.2 AA contrast using tokens only

**Acceptance**
<!-- id: REQ-DCLEAN-024.A1 -->
- Body and secondary text (`--color-text`, `--color-neutral-800`, `--color-neutral-700`) reach at least 4.5:1 against both `--color-bg` and `--color-surface`.
<!-- id: REQ-DCLEAN-024.A2 -->
- Text set in an accent at body size uses a deep ramp step (`--color-accent-700` or deeper), never `--color-accent` itself. The design system rates the accent-on-ground pair for large text and chrome only.
<!-- id: REQ-DCLEAN-024.A3 -->
- `--text-muted` is not used for any text that conveys required information unless it meets 4.5:1.
<!-- id: REQ-DCLEAN-024.A4 -->
- Focus rings and selection-control borders reach at least 3:1 against their adjacent background.
<!-- id: REQ-DCLEAN-024.A5 -->
- Any token added for error, danger or disabled states is defined in the GUI token sheet as an alias of an Organic ramp step and meets the same ratios. No literal colour appears in component code.

<!-- id: REQ-DCLEAN-025 -->
## Motion MUST respect the user's reduced-motion preference

**Acceptance**
<!-- id: REQ-DCLEAN-025.A1 -->
- Ring, bar and chevron transitions use `--motion-ring`, `--motion-bar` and `--motion-chevron` respectively.
<!-- id: REQ-DCLEAN-025.A2 -->
- With reduced motion requested, these transitions are disabled and state changes apply immediately.

<!-- id: REQ-DCLEAN-026 -->
## The window MUST open at the design size and MUST NOT shrink below it

The snapshot is a fixed 1200 × 780 window with no breakpoints, so the GUI is a fixed-layout desktop window.

**Acceptance**
<!-- id: REQ-DCLEAN-026.A1 -->
- The window opens at 1200 × 780 logical pixels and its minimum size is 1200 × 780.
<!-- id: REQ-DCLEAN-026.A2 -->
- When the window is larger than the minimum, the sidebar stays at `--layout-sidebar-w` and the main area takes the extra space. The idle and done two-column layouts keep the ring rail at its design width.
<!-- id: REQ-DCLEAN-026.A3 -->
- The results list scrolls independently of the header and footer at every window size.

<!-- id: REQ-DCLEAN-027 -->
## The component MUST consume a defined data shape derived from a disky snapshot

Inputs:

- `disk`: volume name, capacity bytes, free bytes (physical).
- `goal_bytes`.
- `last_scan_at`: timestamp or null.
- `preferences`: trash-by-default, safe-only preselection, notifications enabled, assistant on.
- `categories[]`: each has `id`, `name`, `description`, `safety` (`safe` or `review`), `bytes` (physical) and `items[]`; each item has `name`, `path` (absolute) and `bytes`.

Scan progress events: `fraction` 0..1, `current_path`, `completed_category_ids[]`.

Clean result: `mode` (`trash` or `delete`), `freed_bytes`, `succeeded[]` paths, `failed[]` entries of {path, reason}, and `history_entry`.

**Acceptance**
<!-- id: REQ-DCLEAN-027.A1 -->
- All byte values are integers in physical bytes. Formatted strings are produced only at render time.
<!-- id: REQ-DCLEAN-027.A2 -->
- Category `safety` is one of exactly two values; any other value is a contract violation and rejected at the input boundary.
<!-- id: REQ-DCLEAN-027.A3 -->
- `categories[].bytes` equals the sum of its `items[].bytes`.
<!-- id: REQ-DCLEAN-027.A4 -->
- The category set and its safety classification are produced by the cleanup engine (REQ-DCLEAN-033), not hard-coded in the component.

<!-- id: REQ-DCLEAN-028 -->
## Displayed sizes MUST be formatted consistently from exact bytes

**Acceptance**
<!-- id: REQ-DCLEAN-028.A1 -->
- Sizes of at least one GB show one decimal below 100 GB and no decimals at or above 100 GB; sizes below one GB show whole MB, with a minimum of "1 MB" for any non-zero value.
<!-- id: REQ-DCLEAN-028.A2 -->
- One unit base (decimal or binary) is used consistently, and it matches what Finder reports for the volume: decimal (1 GB = 10^9 bytes).
<!-- id: REQ-DCLEAN-028.A3 -->
- No displayed figure is invented or estimated. Every size traces to a byte value in the input data.
<!-- id: REQ-DCLEAN-028.A4 -->
- Numeric columns use tabular figures, so totals align.

<!-- id: REQ-DCLEAN-029 -->
## Invalid or stale input data MUST fail safe

**Acceptance**
<!-- id: REQ-DCLEAN-029.A1 -->
- If a selected item's path no longer exists at clean time, it is reported in `failed[]` with reason "missing", and the clean proceeds with the remaining items.
<!-- id: REQ-DCLEAN-029.A2 -->
- If the snapshot is older than a staleness threshold, the results phase warns before cleaning. The threshold is 24 hours.
<!-- id: REQ-DCLEAN-029.A3 -->
- A path inside a user-excluded folder (Settings, "Never touch these folders") is never listed or cleaned.
<!-- id: REQ-DCLEAN-029.A4 -->
- Malformed input (missing required fields, negative bytes) renders the error state from REQ-DCLEAN-018, not a partial list.

<!-- id: REQ-DCLEAN-030 -->
## The component MUST meet render and interaction latency budgets

**Acceptance**
<!-- id: REQ-DCLEAN-030.A1 -->
- The idle phase is interactive within the initial-render budget after launch, using the newest existing snapshot without re-scanning, within 1 s.
<!-- id: REQ-DCLEAN-030.A2 -->
- Selection toggles, expand/collapse, and the footer total update within 100 ms.
<!-- id: REQ-DCLEAN-030.A3 -->
- The results phase renders categories with up to 200 items per category without blocking input; beyond 200, the list shows the largest 200 and a "+N more" line.
<!-- id: REQ-DCLEAN-030.A4 -->
- Computing results from an existing snapshot completes within the same order as `disky cleanup` today (sub-second on a 1.77 M-row snapshot).
<!-- id: REQ-DCLEAN-030.A5 -->
- Progress UI updates at least as often as the scan's progress-event cadence, and never more than once per frame.

<!-- id: REQ-DCLEAN-031 -->
## Visual styling MUST reference design tokens by name only

**Acceptance**
<!-- id: REQ-DCLEAN-031.A1 -->
- Component code contains no literal colour, font family, pixel size or duration that a token in `tokens.css` already carries.
<!-- id: REQ-DCLEAN-031.A2 -->
- Headings use `--font-heading` and body text uses `--font-body`; screen titles use `--fs-screen-title` / `--fs-screen-hero`, and the ring figure uses `--fs-ring-number`.
<!-- id: REQ-DCLEAN-031.A3 -->
- Elevation uses the `--shadow-*` tokens exactly where the snapshot uses them (window, dialog, notification, cards), and nowhere else.
<!-- id: REQ-DCLEAN-031.A4 -->
- The only token this spec adds is `--color-danger` (alias of `--color-accent-800`). It is defined before the components that use it.

<!-- id: REQ-DCLEAN-032 -->
## The app shell MUST reproduce the snapshot's window chrome, sidebar and screen frame

The shell is the frame around every screen: a title bar of `--layout-titlebar-h`, a sidebar of `--layout-sidebar-w` with the brand, seven navigation entries (Smart Clean, Assistant, Space Map, Large files, Duplicates, History, Settings), the assistant badge, and the disk-usage meter (REQ-DCLEAN-002).

**Acceptance**
<!-- id: REQ-DCLEAN-032.A1 -->
- At 1200 × 780, a screenshot of each Smart Clean phase matches the corresponding `snapshot.html` screen. The only allowed differences are the brand text, live data in place of fixture data, and font antialiasing. The comparison threshold is 1% of differing pixels.
<!-- id: REQ-DCLEAN-032.A2 -->
- The brand text reads "disky" everywhere the snapshot shows "Diskee", including the window title.
<!-- id: REQ-DCLEAN-032.A3 -->
- The Assistant, Space Map, Large files, Duplicates, History and Settings entries render exactly as in the snapshot. Activating one opens a placeholder screen with the screen's title and the line "Coming soon". Each is replaced when its own spec is implemented.
<!-- id: REQ-DCLEAN-032.A4 -->
- Fonts (Caprasimo, Figtree) and Lucide icons are bundled with the app, so it renders identically with no network access.

<!-- id: REQ-DCLEAN-033 -->
## The cleanup engine MUST classify the design's system-level categories as safe or review

The engine currently finds developer artefacts (`node_modules`, `target`, …). It MUST also produce the snapshot's categories: system caches, app caches, temporary files, logs, the Trash, app leftovers and old downloads. Each carries a `safety` of `safe` or `review`.

implementations:
  - src/cleanup.rs:scan_with
  - src/cleanup.rs:apply
  - src/cleanup.rs:InstalledApps
  - src/cleanup.rs:CleanupHit
  - src/cleanup.rs:Safety
  - src/cleanup.rs:summarise
  - src/cleanup.rs:CATEGORIES
  - src/cleanup.rs:category
  - src/cleanup.rs:in_scope
  - src/cleanup.rs:location_roots

verifies:
  - tests/lib_integration.rs:cleanup_reports_system_categories
  - src/cleanup.rs:dev_categories_keep_names_and_are_review
  - src/cleanup.rs:safety_serialises_lowercase
  - src/cleanup.rs:category_names_are_unique
  - src/cleanup.rs:location_safety_classes_match_contract
  - tests/lib_integration.rs:cleanup_respects_root_and_ignore
  - src/cleanup.rs:in_scope_rejects_paths_outside_root
  - src/cleanup.rs:in_scope_rejects_ignored_and_excluded_paths
  - tests/lib_integration.rs:cleanup_is_fast_on_large_snapshot
  - tests/lib_integration.rs:cleanup_missing_location_is_silent

**Acceptance**
<!-- id: REQ-DCLEAN-033.A1 -->
- `disky cleanup --format json` emits each hit with a `category` and a `safety` field. Existing developer-artefact categories keep their names, and their `safety` is `review`.
<!-- id: REQ-DCLEAN-033.A2 -->
- Each new category's locations and its safety class are defined in one table in the engine, not in `gui/`. The GUI shows exactly what the engine returns.
<!-- id: REQ-DCLEAN-033.A3 -->
- Paths inside a user-excluded folder, or outside the scanned root, are never returned as hits.
<!-- id: REQ-DCLEAN-033.A4 -->
- The extension keeps the existing performance bound: under 1 s on a 1.77 M-row snapshot (`cleanup_is_fast_on_large_snapshot`).
<!-- id: REQ-DCLEAN-033.A5 -->
- A category location that doesn't exist on the machine produces no hit and no error.

<!-- id: REQ-DCLEAN-034 -->
## Every applied clean MUST be recorded in a history log that Undo and restore read

implementations:
  - src/history.rs:Entry
  - src/history.rs:Item
  - src/history.rs:ItemStatus
  - src/history.rs:RestoreReport
  - src/history.rs:log_path
  - src/history.rs:load
  - src/history.rs:list
  - src/history.rs:append
  - src/history.rs:restore
  - src/history.rs:with_lock
  - src/cleanup.rs:apply
  - src/cleanup.rs:apply_with
  - src/cleanup.rs:ApplyOutcome
  - src/render.rs:history
  - src/render.rs:restore

verifies:
  - src/history.rs:append_then_list_is_newest_first
  - src/history.rs:append_keeps_ids_unique
  - src/history.rs:restore_moves_items_back_and_marks_entry
  - src/history.rs:restore_never_overwrites_and_retries_after_conflict_clears
  - src/history.rs:restore_reports_missing_and_restores_the_rest
  - src/history.rs:restore_refuses_delete_entries
  - src/history.rs:restore_refuses_fully_restored_and_unknown_entries
  - src/history.rs:log_survives_reload_from_disk
  - src/history.rs:concurrent_appends_lose_nothing
  - src/cleanup.rs:apply_trash_records_one_history_entry
  - src/cleanup.rs:apply_delete_records_entry_without_trash_paths
  - src/cleanup.rs:apply_with_nothing_handled_writes_no_entry
  - tests/agentic.rs:history_and_restore_round_trip_through_the_cli
  - tests/agentic.rs:history_is_empty_and_restore_unknown_id_is_not_found

**Acceptance**
<!-- id: REQ-DCLEAN-034.A1 -->
- Every applied clean, from the CLI or the GUI, appends one entry: an id, a timestamp, a mode (`trash` or `delete`), the categories, the freed bytes, and for each item its original path, its Trash path (null in delete mode) and its bytes.
<!-- id: REQ-DCLEAN-034.A2 -->
- `disky history --format json` lists the entries newest first, and `disky restore <id>` moves every trashed item of that entry back to its original path and marks the entry restored.
<!-- id: REQ-DCLEAN-034.A3 -->
- Restore never overwrites an existing file at the original path. That item is reported as a conflict and left in the Trash.
<!-- id: REQ-DCLEAN-034.A4 -->
- An item that is no longer at its recorded Trash path is reported as "no longer in Trash", and the other items are still restored.
<!-- id: REQ-DCLEAN-034.A5 -->
- A clean in delete mode records an entry that can't be restored, and restore refuses it with a clear error.
<!-- id: REQ-DCLEAN-034.A6 -->
- The log lives in disky's data directory, next to the snapshots, and survives app restarts.

