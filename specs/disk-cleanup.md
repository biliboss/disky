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

implementations:
  - gui/src-tauri/src/main.rs:load_cleanup_view
  - gui/src-tauri/src/main.rs:scan
  - gui/src-tauri/src/main.rs:stop_scan
  - gui/src-tauri/src/main.rs:clean
  - gui/src/lib/engine.ts:loadCleanupView
  - gui/src/lib/engine.ts:scan
  - gui/src/lib/engine.ts:clean
  - gui/src/lib/engine.ts:onScanProgress
  - src/contract.rs:load
  - src/contract.rs:clean
  - src/contract.rs:view_hits
  - src/scan.rs:run_observed
  - gui/scripts/lint-boundary.mjs:check

verifies:
  - tests/agentic.rs:gui_categories_match_cli_cleanup_totals
  - tests/lib_integration.rs:contract_totals_match_cleanup_summary
  - src/contract.rs:select_reviewed_refuses_unreviewed_paths

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

implementations:
  - src/disk.rs:disk_usage
  - src/contract.rs:disk_near
  - src/contract.rs:clean
  - src/contract.rs:CleanResult
  - src/render.rs:cleanup
  - gui/src/lib/meter.ts:goalPos
  - gui/src/lib/meter.ts:goalStatus
  - gui/src/lib/meter.ts:ringDash
  - gui/src/lib/diskState.ts:setFromView
  - gui/src/lib/diskState.ts:setFromClean
  - gui/src/lib/engine.ts:clean
  - gui/src/lib/contract.ts:parseCleanResult
  - gui/src/shell/AppShell.tsx:AppShell
  - gui/src/shell/DiskMeter.tsx:DiskMeter
  - gui/src/shell/GoalTag.tsx:GoalTag
  - gui/src/shell/UsageRing.tsx:UsageRing

verifies:
  - src/disk.rs:disk_usage_reports_physical_volume_space
  - src/contract.rs:clean_result_carries_post_clean_disk
  - src/contract.rs:disk_near_walks_up_to_an_existing_ancestor
  - tests/agentic.rs:cleanup_apply_envelope_carries_post_clean_disk
  - gui/src/lib/contract.test.ts:cleanResultWithoutDiskIsRejected
  - gui/src/shell/__tests__/meter.test.ts:goalPosMatchesSnapshotFixture
  - gui/src/shell/__tests__/meter.test.ts:goalStatusIsBelowOnlyStrictlyUnderTheGoal
  - gui/src/shell/__tests__/meter.test.ts:ringDashIsTheUsedShareOfTheCircumference
  - gui/src/shell/__tests__/GoalTag.test.tsx:goalTagReadsBelowStrictlyUnderGoal
  - gui/src/shell/__tests__/GoalTag.test.tsx:goalTagReadsMetAtOrAboveGoal
  - gui/src/shell/__tests__/UsageRing.test.tsx:ringLabelShowsFreeAndCapacity
  - gui/src/shell/__tests__/UsageRing.test.tsx:ringFillClassFollowsPhase
  - gui/src/shell/__tests__/UsageRing.test.tsx:ringAndBarUseTheSpecifiedTokens
  - gui/src/shell/__tests__/Shell.test.tsx:meterShowsLiveVolumeAndFreeSpace
  - gui/src/shell/__tests__/Shell.test.tsx:goalMarkerFollowsLiveGoalPos
  - gui/src/shell/__tests__/Shell.test.tsx:meterFollowsACleanWithoutRemount
  - gui/src/screens/__tests__/SmartClean.test.tsx:cleanUpdatesRingTagAndSidebar

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

implementations:
  - gui/src/screens/SmartClean.tsx:SmartClean
  - gui/src/screens/smartClean/Idle.tsx:Idle
  - gui/src/screens/smartClean/phase.ts:startScan
  - gui/src/screens/smartClean/phase.ts:scanProgress
  - gui/src/screens/smartClean/phase.ts:scanFinished
  - gui/src/screens/smartClean/phase.ts:scanStopped
  - gui/src/screens/smartClean/idleCopy.ts:idleDescription
  - gui/src/screens/smartClean/idleCopy.ts:lastScanKicker
  - gui/src/screens/smartClean/idleCopy.ts:assistantShortcut
  - gui/src/screens/smartClean/idleCopy.ts:largeFilesShortcut
  - gui/src/lib/format.ts:relativeTime
  - gui/src/lib/contract.ts:parseCleanupView
  - src/contract.rs:large_files_count
  - src/contract.rs:empty_view
  - src/contract.rs:load
  - gui/src-tauri/src/main.rs:load_cleanup_view

verifies:
  - gui/src/screens/__tests__/phase.test.ts:startScanResetsProgressCollapsesAndDismisses
  - gui/src/screens/__tests__/phase.test.ts:startScanIsIgnoredWhileBusy
  - gui/src/screens/__tests__/phase.test.ts:scanProgressNeverGoesBackwards
  - gui/src/screens/__tests__/phase.test.ts:stoppedScanIgnoresALateFinish
  - gui/src/screens/__tests__/idleCopy.test.ts:descriptionFollowsTrashMode
  - gui/src/screens/__tests__/idleCopy.test.ts:kickerReadsNoScansYetWithoutASnapshot
  - gui/src/screens/__tests__/idleCopy.test.ts:kickerUsesTheSnapshotTimestamp
  - gui/src/screens/__tests__/idleCopy.test.ts:relativeTimeUnits
  - gui/src/screens/__tests__/idleCopy.test.ts:assistantShortcutCopy
  - gui/src/screens/__tests__/idleCopy.test.ts:largeFilesCopyComesFromTheCount
  - gui/src/screens/__tests__/idleCopy.test.ts:headingMatchesTheSnapshot
  - gui/src/screens/__tests__/SmartClean.test.tsx:idleShowsHeadingDescriptionActionsAndShortcuts
  - gui/src/screens/__tests__/SmartClean.test.tsx:startScanEntersScanningAtZero
  - gui/src/screens/__tests__/SmartClean.test.tsx:stopReturnsToIdleWithoutResults
  - gui/src/screens/__tests__/SmartClean.test.tsx:kickerShowsNoScansYetWithoutASnapshot
  - gui/src/screens/__tests__/SmartClean.test.tsx:kickerShowsTheSnapshotAge
  - gui/src/screens/__tests__/SmartClean.test.tsx:assistantShortcutWatchesTheGoalWithNoSuggestions
  - gui/src/screens/__tests__/SmartClean.test.tsx:largeFilesCountComesFromTheEngine
  - gui/src/screens/__tests__/SmartClean.test.tsx:descriptionIsTruthfulWhenTrashIsOff
  - gui/src/screens/__tests__/SmartClean.test.tsx:exploreDiskOpensSpaceMap
  - gui/src/screens/__tests__/SmartClean.test.tsx:shortcutsOpenTheirScreens
  - gui/src/lib/contract.test.ts:largeFilesCountMustBeAWholeCount
  - src/contract.rs:large_files_count_uses_physical_bytes_at_2gb
  - src/contract.rs:empty_view_has_no_scan_and_live_disk
  - gui/tests/visual/shell.spec.ts:idlePhaseMatchesSnapshot

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

The rows list every category the engine checks (`CleanupView.catalogue`, built from `cleanup::CATEGORIES`), so a first scan shows them all. A completed category with no hits reads "Nothing found" and shows no size.

**Known limitation.** The engine classifies categories after the walk finishes, so every row stays "Checking…" until the scan's final event and then all rows complete together. Completing categories one at a time while the walk runs is a follow-up.

implementations:
  - gui/src/screens/smartClean/Scanning.tsx:Scanning
  - gui/src/screens/smartClean/phase.ts:scanProgress
  - gui/src/screens/smartClean/phase.ts:scanStopped
  - gui/src/screens/smartClean/phase.ts:categoryRowStatus
  - gui/src/screens/SmartClean.tsx:SmartClean
  - gui/src/lib/contract.ts:parseScanProgress
  - gui/src/lib/contract.ts:parseCleanupView
  - src/contract.rs:catalogue
  - src/contract.rs:scan_progress
  - src/contract.rs:scan_finished
  - gui/src-tauri/src/main.rs:scan
  - gui/src-tauri/src/main.rs:stop_scan

verifies:
  - gui/src/screens/__tests__/phase.test.ts:scanProgressNeverGoesBackwards
  - gui/src/screens/__tests__/phase.test.ts:progressReachesOneOnlyWhenTheScanFinishes
  - gui/src/screens/__tests__/phase.test.ts:completedCategoriesOnlyGrow
  - gui/src/screens/__tests__/phase.test.ts:currentPathSurvivesAnEmptyPath
  - gui/src/screens/__tests__/phase.test.ts:stoppedScanIgnoresALateFinish
  - gui/src/screens/__tests__/phase.test.ts:categoryRowStatusCoversEveryState
  - gui/src/screens/__tests__/SmartClean.test.tsx:scanningShowsHeadingPathAndTruncatedTooltip
  - gui/src/screens/__tests__/SmartClean.test.tsx:progressBarFollowsEventsAndNeverDecreases
  - gui/src/screens/__tests__/SmartClean.test.tsx:rowsShowSizesOnlyOnceTheirCategoryIsComplete
  - gui/src/screens/__tests__/SmartClean.test.tsx:completionEntersResultsWithoutAPause
  - gui/src/screens/__tests__/SmartClean.test.tsx:stopDiscardsALateResultAndModifiesNothing
  - gui/src/lib/contract.test.ts:catalogueAndCompletedBytesAreStrict
  - src/contract.rs:scan_finished_completes_every_category
  - src/contract.rs:catalogue_lists_every_engine_category
  - gui/tests/visual/shell.spec.ts:scanningPhaseMatchesSnapshot

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

The icon is the Lucide icon name the engine table gives each category (`cleanup::Category::icon`, carried on `CategoryView` and `CatalogueEntry` and listed by `disky schema`). The GUI resolves it by icon name, never by category id, and falls back to a folder for a name it does not know.

implementations:
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/smartClean/resultsCopy.ts:resultsHeading
  - gui/src/screens/smartClean/resultsCopy.ts:resultsKicker
  - gui/src/screens/smartClean/resultsCopy.ts:safetyTag
  - gui/src/screens/smartClean/resultsCopy.ts:disclosureLabel
  - gui/src/screens/smartClean/phase.ts:toggleExpanded
  - gui/src/shell/iconByName.ts:iconByName
  - gui/src/lib/contract.ts:parseCleanupView
  - gui/src/screens/SmartClean.tsx:SmartClean
  - src/contract.rs:categories_from_hits
  - src/contract.rs:catalogue
  - src/schema.rs:cleanup_categories

verifies:
  - gui/src/screens/__tests__/phase.test.ts:startScanClearsOpenedByAndSelection
  - gui/src/screens/__tests__/phase.test.ts:toggleExpandedOpensSwitchesAndCollapses
  - gui/src/screens/__tests__/phase.test.ts:toggleExpandedIsIgnoredOutsideResults
  - gui/src/screens/__tests__/resultsCopy.test.ts:headingTotalIsTheSumOfListedCategories
  - gui/src/screens/__tests__/resultsCopy.test.ts:kickerHasBothFormsSingularAndPlural
  - gui/src/screens/__tests__/resultsCopy.test.ts:safetyTagClasses
  - gui/src/screens/__tests__/resultsCopy.test.ts:disclosureIsNamedByItsCategory
  - gui/src/screens/__tests__/resultsCopy.test.ts:itemSizesAddUpToTheCategorySize
  - gui/src/screens/__tests__/SmartClean.test.tsx:rendersExactlyTheEngineCategories
  - gui/src/screens/__tests__/SmartClean.test.tsx:rowsAndItemsComeOnlyFromTheScan
  - gui/src/screens/__tests__/SmartClean.test.tsx:rowCarriesSelectionIconNameTagDescriptionSizeAndDisclosure
  - gui/src/screens/__tests__/SmartClean.test.tsx:oneCategoryExpandedAtATime
  - gui/src/screens/__tests__/SmartClean.test.tsx:chevronRotatesWithTheMotionToken
  - gui/src/screens/__tests__/SmartClean.test.tsx:longPathIsEllipsisedWithTooltipAndAccessibleName
  - gui/src/shell/__tests__/iconByName.test.ts:everyEngineIconHasItsOwnGlyph
  - gui/src/shell/__tests__/iconByName.test.ts:unknownNamesFallBackToAFolder
  - gui/src/lib/contract.test.ts:categoryAndCatalogueIconsAreRequired
  - src/cleanup.rs:every_category_names_a_lucide_icon
  - src/contract.rs:category_ids_come_from_engine_table
  - src/contract.rs:catalogue_lists_every_engine_category
  - src/schema.rs:cleanup_categories_carry_label_and_description
  - gui/tests/visual/shell.spec.ts:resultsPhaseMatchesSnapshot

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

implementations:
  - gui/src/screens/smartClean/phase.ts:scanFinished
  - gui/src/screens/smartClean/phase.ts:preselect
  - gui/src/screens/smartClean/phase.ts:toggleSelected
  - gui/src/screens/smartClean/phase.ts:selectSafeOnly
  - gui/src/screens/smartClean/phase.ts:selectAll
  - gui/src/screens/smartClean/resultsCopy.ts:selectedBytes
  - gui/src/screens/smartClean/resultsCopy.ts:cleanLabel
  - gui/src/screens/smartClean/resultsCopy.ts:safetyNote
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/screens/__tests__/phase.test.ts:safeOnlyPreferencePreselectsExactlyTheSafeCategories
  - gui/src/screens/__tests__/phase.test.ts:safeOnlyAndSelectAllActions
  - gui/src/screens/__tests__/phase.test.ts:toggleSelectedFlipsOnlyThatCategory
  - gui/src/screens/__tests__/resultsCopy.test.ts:cleanLabelFollowsTheSelectedTotal
  - gui/src/screens/__tests__/resultsCopy.test.ts:safetyNoteFollowsTrashByDefault
  - gui/src/screens/__tests__/SmartClean.test.tsx:safeOnlyPreferencePreselectsSafeCategories
  - gui/src/screens/__tests__/SmartClean.test.tsx:safeOnlyOffPreselectsEverything
  - gui/src/screens/__tests__/SmartClean.test.tsx:safeOnlyAndSelectAllButtons
  - gui/src/screens/__tests__/SmartClean.test.tsx:togglingFlipsOnlyThatCategoryAndUpdatesTheTotal
  - gui/src/screens/__tests__/SmartClean.test.tsx:footerNoteFollowsTrashByDefault
  - gui/src/screens/__tests__/SmartClean.test.tsx:noSelectNoneAction
  - gui/src/screens/__tests__/SmartClean.test.tsx:cleanIsDisabledWhenNothingIsSelected
  - gui/src/screens/__tests__/SmartClean.test.tsx:cancelReturnsHome

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

implementations:
  - gui/src/screens/smartClean/ConfirmClean.tsx:ConfirmClean
  - gui/src/screens/smartClean/phase.ts:openConfirm
  - gui/src/screens/smartClean/phase.ts:cancelConfirm
  - gui/src/screens/smartClean/phase.ts:confirmClean
  - gui/src/screens/smartClean/resultsCopy.ts:confirmTitle
  - gui/src/screens/smartClean/resultsCopy.ts:modeNote
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/screens/__tests__/phase.test.ts:openConfirmNeedsResultsAndASelection
  - gui/src/screens/__tests__/phase.test.ts:cancelConfirmKeepsTheSelection
  - gui/src/screens/__tests__/phase.test.ts:confirmCleanEntersCleaningOnlyFromTheDialog
  - gui/src/screens/__tests__/phase.test.ts:selectionIsFrozenWhileConfirming
  - gui/src/screens/__tests__/resultsCopy.test.ts:confirmTitleTotalEqualsTheFooterTotal
  - gui/src/screens/__tests__/resultsCopy.test.ts:modeNoteCoversBothModes
  - gui/src/screens/__tests__/resultsCopy.test.ts:initialModeFollowsTrashByDefault
  - gui/src/screens/__tests__/SmartClean.test.tsx:cleanOpensTheDialogTitledWithTheTotal
  - gui/src/screens/__tests__/SmartClean.test.tsx:nothingIsCleanedBeforeCleanNow
  - gui/src/screens/__tests__/SmartClean.test.tsx:modeStartsAtTheTrashPreference
  - gui/src/screens/__tests__/SmartClean.test.tsx:noteFollowsTheChosenMode
  - gui/src/screens/__tests__/SmartClean.test.tsx:cancelEscapeAndBackdropCloseWithTheSelectionUnchanged
  - gui/src/screens/__tests__/SmartClean.test.tsx:dialogListsExactlyTheSelectedCategories
  - gui/src/screens/__tests__/SmartClean.test.tsx:deleteModeGivesCleanNowTheDangerFill
  - gui/src/screens/__tests__/SmartClean.test.tsx:dangerAndBackdropUseTheirTokens
  - gui/src/screens/__tests__/SmartClean.test.tsx:modeSwitchIsARadiogroupWithArrowKeys
  - gui/tests/visual/shell.spec.ts:confirmDialogMatchesSnapshot

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

implementations:
  - src/cleanup.rs:apply_observed
  - src/contract.rs:clean_observed
  - src/contract.rs:CleanProgress
  - src/contract.rs:select_reviewed
  - gui/src-tauri/src/main.rs:clean
  - gui/src/lib/contract.ts:parseCleanProgress
  - gui/src/lib/engine.ts:onCleanProgress
  - gui/src/screens/smartClean/phase.ts:confirmClean
  - gui/src/screens/smartClean/phase.ts:cleanProgress
  - gui/src/screens/smartClean/cleaningCopy.ts:stepLabel
  - gui/src/screens/smartClean/Cleaning.tsx:Cleaning
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - src/cleanup.rs:apply_observed_reports_each_hit_in_order
  - src/contract.rs:clean_reports_progress_per_item_in_selection_order
  - src/contract.rs:select_reviewed_keeps_the_callers_order
  - gui/src/lib/contract.test.ts:cleanProgressIsValidated
  - gui/src/screens/__tests__/phase.test.ts:confirmCleanStoresModeAndFirstCategory
  - gui/src/screens/__tests__/phase.test.ts:cleanProgressNeverGoesBackwardsAndFollowsTheCategory
  - gui/src/screens/__tests__/phase.test.ts:cleanProgressIsIgnoredOutsideCleaning
  - gui/src/screens/__tests__/cleaningCopy.test.ts:stepVerbFollowsTheMode
  - gui/src/screens/__tests__/SmartClean.test.tsx:cleaningShowsHeadingStepAndBar
  - gui/src/screens/__tests__/SmartClean.test.tsx:permanentCleanSaysDeleting
  - gui/src/screens/__tests__/SmartClean.test.tsx:cleanFillUsesAccent2
  - gui/src/screens/__tests__/SmartClean.test.tsx:nothingIsCleanedBeforeCleanNow
  - gui/tests/visual/shell.spec.ts:cleaningPhaseMatchesSnapshot

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

implementations:
  - src/contract.rs:CleanResult
  - src/contract.rs:clean_title
  - src/contract.rs:RestoreResult
  - src/contract.rs:restore
  - src/history.rs:Entry
  - src/cleanup.rs:apply_observed
  - src/render.rs:history
  - gui/src-tauri/src/main.rs:clean
  - gui/src-tauri/src/main.rs:restore
  - gui/src/lib/engine.ts:clean
  - gui/src/lib/engine.ts:restore
  - gui/src/lib/contract.ts:parseCleanResult
  - gui/src/lib/contract.ts:parseRestoreResult
  - gui/src/lib/diskState.ts:setDisk
  - gui/src/screens/smartClean/phase.ts:cleanFinished
  - gui/src/screens/smartClean/phase.ts:doneHome
  - gui/src/screens/smartClean/doneCopy.ts:doneHeading
  - gui/src/screens/smartClean/doneCopy.ts:doneNote
  - gui/src/screens/smartClean/doneCopy.ts:canUndo
  - gui/src/screens/smartClean/doneCopy.ts:undoNotices
  - gui/src/screens/smartClean/Done.tsx:Done
  - gui/src/shell/UsageRing.tsx:UsageRing
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - src/contract.rs:clean_title_follows_who_opened_the_review
  - src/contract.rs:undo_restores_a_trash_clean_and_reports_missing
  - src/history.rs:entries_without_a_title_read_as_cli_cleanups
  - src/cleanup.rs:apply_trash_records_one_history_entry
  - gui/src/lib/contract.test.ts:restoreResultParsesTheReportAndDisk
  - gui/src/lib/contract.test.ts:cleanResultParsesWithAndWithoutEntry
  - gui/src/screens/__tests__/phase.test.ts:cleanFinishedEntersDone
  - gui/src/screens/__tests__/phase.test.ts:doneHomeReturnsToIdle
  - gui/src/screens/__tests__/cleaningCopy.test.ts:doneCopyFollowsTheMode
  - gui/src/screens/__tests__/cleaningCopy.test.ts:undoAndFailureCopy
  - gui/src/screens/__tests__/SmartClean.test.tsx:doneReportsReclaimedBytesNoteAndActions
  - gui/src/screens/__tests__/SmartClean.test.tsx:permanentDoneHasNoUndo
  - gui/src/screens/__tests__/SmartClean.test.tsx:backHomeAndViewHistoryNavigate
  - gui/src/screens/__tests__/SmartClean.test.tsx:undoRestoresUpdatesTheMeterAndReportsMissing
  - gui/src/screens/__tests__/SmartClean.test.tsx:undoShowsBusyOnlyPastTheThreshold
  - gui/src/screens/__tests__/SmartClean.test.tsx:failedUndoStaysOnDoneWithTheCause
  - gui/tests/visual/shell.spec.ts:donePhaseMatchesSnapshot

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

implementations:
  - gui/src/screens/smartClean/phase.ts:safeTotal
  - gui/src/screens/smartClean/phase.ts:notificationVisible
  - gui/src/screens/smartClean/phase.ts:dismissNotification
  - gui/src/screens/smartClean/phase.ts:reviewFromNotification
  - gui/src/screens/smartClean/idleCopy.ts:notificationText
  - gui/src/screens/smartClean/GoalNotification.tsx:GoalNotification
  - gui/src/screens/smartClean/Idle.tsx:Idle
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/screens/__tests__/phase.test.ts:notificationVisibleOnlyWhenEveryConditionHolds
  - gui/src/screens/__tests__/phase.test.ts:notificationHiddenWithoutAScanOrSafeTotal
  - gui/src/screens/__tests__/phase.test.ts:safeTotalSumsTheListedSafeCategories
  - gui/src/screens/__tests__/phase.test.ts:dismissNotificationOnlyActsOnIdle
  - gui/src/screens/__tests__/phase.test.ts:reviewFromNotificationSelectsSafeAndOpensTheDialog
  - gui/src/screens/__tests__/idleCopy.test.ts:notificationCopyMatchesTheSnapshot
  - gui/src/screens/__tests__/SmartClean.test.tsx:notificationShowsOnIdleBelowGoal
  - gui/src/screens/__tests__/SmartClean.test.tsx:notificationHiddenUnlessEveryConditionHolds
  - gui/src/screens/__tests__/SmartClean.test.tsx:laterHidesItForTheSession
  - gui/src/screens/__tests__/SmartClean.test.tsx:reviewCleanupOpensResultsWithSafeSelectedAndTheDialog
  - gui/src/screens/__tests__/SmartClean.test.tsx:notificationDoesNotTakeFocus
  - gui/src/screens/__tests__/SmartClean.test.tsx:aScanDismissesIt
  - gui/tests/visual/shell.spec.ts:idleNotificationMatchesSnapshot

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
<!-- id: REQ-DCLEAN-010.A6 -->
- With a Safe total of zero, the notification is not shown: there is nothing to offer. The snapshot always has Safe bytes; this is a gap-fill.

<!-- id: REQ-DCLEAN-011 -->
## Navigation MUST be blocked during scanning and cleaning, and blocked entries MUST look disabled

implementations:
  - gui/src/screens/smartClean/phase.ts:navBlocked
  - gui/src/screens/smartClean/phase.ts:leaveViaNav
  - gui/src/screens/smartClean/store.ts:isNavBlocked
  - gui/src/shell/NavItem.tsx:NavItem
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/screens/__tests__/phase.test.ts:navIsBlockedWhileScanningOrCleaning
  - gui/src/screens/__tests__/phase.test.ts:leavingDoneViaNavResetsToIdle
  - gui/src/screens/__tests__/SmartClean.test.tsx:navIsBlockedAndDisabledWhileScanning
  - gui/src/screens/__tests__/SmartClean.test.tsx:navIsBlockedAndDisabledWhileCleaning
  - gui/src/screens/__tests__/SmartClean.test.tsx:disabledAndCurrentEntriesUseTheirTokens
  - gui/src/screens/__tests__/SmartClean.test.tsx:currentEntryIsExposedAsTheCurrentPage
  - gui/src/screens/__tests__/SmartClean.test.tsx:navigatingFromDoneShowsIdle
  - gui/tests/visual/shell.spec.ts:blockedNavIgnoresTheKeyboard

**Acceptance**
<!-- id: REQ-DCLEAN-011.A1 -->
- While scanning or cleaning, activating any navigation entry does not change the screen.
<!-- id: REQ-DCLEAN-011.A2 -->
- While blocked, navigation entries are exposed as disabled to assistive tech and use the disabled treatment (REQ-DCLEAN-016). The snapshot ignores clicks silently; this is a gap-fill.
<!-- id: REQ-DCLEAN-011.A3 -->
- The current screen's entry is marked as current (accent fill per `--color-accent`, text in `--color-on-accent`) and exposed as the current page. The snapshot's `--color-bg` label is 3.0:1 on the accent; `--color-on-accent` (alias of `--color-text`) reaches 4.5:1 (REQ-DCLEAN-024).
<!-- id: REQ-DCLEAN-011.A4 -->
- Leaving the done phase via navigation resets Smart Clean to idle.

<!-- id: REQ-DCLEAN-012 -->
## Every interactive control MUST render a themed default state from design-system classes

implementations:
  - gui/scripts/controlClasses.mjs:buttonClassProblem
  - gui/scripts/controlClasses.mjs:THEMED_EXCEPTIONS
  - gui/scripts/lint-controls.mjs:classesOf
  - gui/scripts/lint-controls.mjs:classesOf
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/smartClean/Results.tsx:CategoryRow
  - gui/src/screens/smartClean/Idle.tsx:Idle

verifies:
  - gui/src/screens/__tests__/controls.test.tsx:everyButtonUsesDesignSystemClasses
  - gui/src/screens/__tests__/controls.test.tsx:checkboxIsThemedAndFlipsChecked
  - gui/src/screens/__tests__/controls.test.tsx:rowsAndCardsUseSurfaceTokens
  - gui/src/styles/lints.test.ts:lintControlsFindsNoUnthemedControl

**Acceptance**
<!-- id: REQ-DCLEAN-012.A1 -->
- Buttons use the design-system button classes (primary, secondary, ghost, icon); no control relies on browser-default styling.
<!-- id: REQ-DCLEAN-012.A2 -->
- Selection controls render unchecked with a `--color-check-border` border (alias of `--color-neutral-600`), and checked with a `--color-accent` fill, a `--color-check-border-checked` edge (alias of `--color-accent-700`) and a `--color-bg` check, sized by `--checkbox-size`. The snapshot's `--color-neutral-400` border is 1.5:1 on the row surface, below REQ-DCLEAN-024.A4.
<!-- id: REQ-DCLEAN-012.A3 -->
- Category rows and cards use `--color-surface` on `--color-bg`, with radius from `--radius-md` / `--radius-card`.

<!-- id: REQ-DCLEAN-013 -->
## Interactive controls MUST show a hover state

implementations:
  - gui/scripts/controlClasses.mjs:STATEFUL_CLASSES
  - gui/scripts/lint-controls.mjs:rules
  - gui/src/shell/NavItem.tsx:NavItem
  - gui/src/screens/smartClean/Results.tsx:CategoryRow

verifies:
  - gui/tests/visual/a11y.spec.ts:controlsShowHoverAndPressedStates
  - gui/src/styles/lints.test.ts:lintControlsFindsNoUnthemedControl

**Acceptance**
<!-- id: REQ-DCLEAN-013.A1 -->
- The primary button hover uses `--color-primary-hover` (alias of `--color-accent-500`). Its label is `--color-on-accent`, which keeps 4.5:1 only on lighter steps, so hover lightens rather than darkens (REQ-DCLEAN-024).
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

implementations:
  - gui/scripts/lint-controls.mjs:rules
  - gui/src/screens/smartClean/ConfirmClean.tsx:ConfirmClean
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/tests/visual/a11y.spec.ts:everyControlShowsTheRingOnTab
  - gui/tests/visual/a11y.spec.ts:pointerFocusShowsNoRing
  - gui/src/styles/lints.test.ts:lintControlsFindsNoUnthemedControl

**Acceptance**
<!-- id: REQ-DCLEAN-014.A1 -->
- Every focusable control shows the design-system focus-visible ring in `--focus-ring-color` (alias of `--color-accent-700`) when focused via keyboard, and no ring when focused by pointer. `--color-accent` is 2.7:1 on `--color-surface`, below REQ-DCLEAN-024.A4.
<!-- id: REQ-DCLEAN-014.A2 -->
- Segmented-control options show the ring on the option wrapper when their hidden radio has keyboard focus. The ring is inset over the option's fill, so it uses `--focus-ring-inset-color` (alias of `--color-text`).
<!-- id: REQ-DCLEAN-014.A3 -->
- No control shows the browser-default focus ring.

<!-- id: REQ-DCLEAN-015 -->
## Interactive controls MUST show a pressed state

implementations:
  - gui/scripts/controlClasses.mjs:STATEFUL_CLASSES
  - gui/src/screens/smartClean/ConfirmClean.tsx:ConfirmClean

verifies:
  - gui/tests/visual/a11y.spec.ts:controlsShowHoverAndPressedStates

**Acceptance**
<!-- id: REQ-DCLEAN-015.A1 -->
- The primary button pressed state uses `--color-primary-active` (alias of `--color-accent-400`), for the reason in REQ-DCLEAN-013.A1.
<!-- id: REQ-DCLEAN-015.A2 -->
- The secondary button pressed state uses `--overlay-active`, and the ghost button uses `--overlay-ghost-active`.
<!-- id: REQ-DCLEAN-015.A3 -->
- The selected segmented option uses `--color-accent` fill with `--color-on-accent` text.

<!-- id: REQ-DCLEAN-016 -->
## Unavailable actions MUST render disabled and MUST NOT activate

implementations:
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/smartClean/Done.tsx:Done
  - gui/src/shell/NavItem.tsx:NavItem
  - gui/src/screens/smartClean/store.ts:isNavBlocked

verifies:
  - gui/src/screens/__tests__/controls.test.tsx:cleanIsDisabledAtZeroAndNeverFires
  - gui/src/screens/__tests__/controls.test.tsx:doneActionsAreDisabledWhileUndoing
  - gui/src/screens/__tests__/controls.test.tsx:disabledTreatmentIsTheDesignSystems
  - gui/tests/visual/a11y.spec.ts:disabledCleanIgnoresPointerAndKeyboard
  - gui/src/screens/__tests__/SmartClean.test.tsx:navIsBlockedAndDisabledWhileScanning
  - gui/tests/visual/shell.spec.ts:blockedNavIgnoresTheKeyboard

**Acceptance**
<!-- id: REQ-DCLEAN-016.A1 -->
- "Clean <total>" is disabled when the selected total is zero.
<!-- id: REQ-DCLEAN-016.A2 -->
- Disabled controls use the design-system disabled treatment (reduced opacity, not-allowed cursor) and ignore pointer and keyboard activation. The treatment is the design system's built-in 45% opacity.
<!-- id: REQ-DCLEAN-016.A3 -->
- Disabled controls are exposed as disabled to assistive tech, and remain discoverable rather than being removed from the reading order.

<!-- id: REQ-DCLEAN-017 -->
## Asynchronous work MUST show a loading state only when it would otherwise feel stalled

implementations:
  - gui/src/lib/busy.ts:useDelayedBusy
  - gui/src/screens/SmartClean.tsx:SmartClean
  - gui/src/screens/smartClean/Scanning.tsx:Scanning

verifies:
  - gui/src/lib/busy.test.ts:busyAppearsOnlyAfterTheThreshold
  - gui/src/lib/busy.test.ts:aShortWaitNeverShowsBusy
  - gui/src/screens/__tests__/SmartClean.test.tsx:slowLoadShowsBusyOnlyAfterTheThreshold
  - gui/src/screens/__tests__/SmartClean.test.tsx:fastLoadNeverShowsBusy
  - gui/src/screens/__tests__/SmartClean.test.tsx:startScanEntersScanningAtZero
  - gui/src/screens/__tests__/SmartClean.test.tsx:completionEntersResultsWithoutAPause

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

implementations:
  - src/scan.rs:run_observed
  - src/db.rs:write_scan_skipped
  - src/db.rs:read_scan_skipped
  - src/contract.rs:load
  - src/contract.rs:EngineError
  - src/contract.rs:clean_result
  - src/cleanup.rs:apply_one
  - gui/src-tauri/src/main.rs:blocking
  - gui/src-tauri/src/main.rs:scan
  - gui/src-tauri/src/main.rs:open_full_disk_access
  - gui/src/lib/contract.ts:EngineError
  - gui/src/lib/contract.ts:parseEngineError
  - gui/src/lib/engine.ts:openFullDiskAccess
  - gui/src/screens/smartClean/errorCopy.ts:scanErrorMessage
  - gui/src/screens/smartClean/errorCopy.ts:canRetry
  - gui/src/screens/smartClean/errorCopy.ts:skippedPaths
  - gui/src/screens/smartClean/doneCopy.ts:cleanFailedMessage
  - gui/src/screens/smartClean/doneCopy.ts:failedHeading
  - gui/src/screens/smartClean/phase.ts:cleanFailed
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/smartClean/Done.tsx:Done
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - tests/lib_integration.rs:scan_counts_permission_skipped_paths
  - src/cleanup.rs:apply_delete_failure_leaves_path_and_proceeds
  - src/contract.rs:partial_clean_frees_only_what_was_reclaimed
  - src/contract.rs:engine_errors_carry_the_exit_slug
  - src/contract.rs:full_disk_access_url_opens_the_privacy_pane
  - gui/src/lib/contract.test.ts:engineErrorsKeepTheExitSlug
  - gui/src/lib/contract.test.ts:skippedPathsMustBeAWholeCount
  - gui/src/screens/__tests__/phase.test.ts:cleanFailedReturnsToResultsWithTheSelection
  - gui/src/screens/__tests__/cleaningCopy.test.ts:errorCopyDispatchesOnTheCode
  - gui/src/screens/__tests__/SmartClean.test.tsx:lockHeldScanNamesTheCauseWithTryAgain
  - gui/src/screens/__tests__/SmartClean.test.tsx:otherScanErrorsKeepTheEngineDetail
  - gui/src/screens/__tests__/SmartClean.test.tsx:skippedPathsAreCountedWithAFullDiskAccessLink
  - gui/src/screens/__tests__/SmartClean.test.tsx:noSkippedNoteWhenNothingWasSkipped
  - gui/src/screens/__tests__/SmartClean.test.tsx:partialCleanListsFailedPathsAndReclaimedBytes
  - gui/src/screens/__tests__/SmartClean.test.tsx:totalFailureReturnsToResultsWithTheSelection
  - gui/src/screens/__tests__/SmartClean.test.tsx:aRejectedCleanAlsoReturnsToResults
  - gui/src/screens/__tests__/SmartClean.test.tsx:errorStatesUseTheDangerToken

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

implementations:
  - gui/src/screens/smartClean/phase.ts:listedCategories
  - gui/src/screens/smartClean/phase.ts:backHome
  - gui/src/screens/smartClean/Results.tsx:Results
  - gui/src/screens/smartClean/Results.tsx:EmptyResults
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/screens/__tests__/phase.test.ts:zeroSizeCategoriesAreNotListed
  - gui/src/screens/__tests__/phase.test.ts:backHomeReturnsToIdleWithNothingSelected
  - gui/src/screens/__tests__/resultsCopy.test.ts:emptyCopyMatchesTheSpec
  - gui/src/screens/__tests__/SmartClean.test.tsx:emptyViewShowsNothingToClean
  - gui/src/screens/__tests__/SmartClean.test.tsx:zeroCategoriesShowTheEmptyStateWithBackToHome
  - gui/src/screens/__tests__/SmartClean.test.tsx:zeroSizeCategoryIsOmitted
  - gui/src/screens/__tests__/SmartClean.test.tsx:expandedCategoryWithNoItemsSaysNoItems

**Acceptance**
<!-- id: REQ-DCLEAN-019.A1 -->
- When the scan yields zero categories, the results phase shows an empty-state message with "Back to home" and no "Clean" action. The copy reads "Nothing worth clearing right now."
<!-- id: REQ-DCLEAN-019.A2 -->
- A category whose size is zero is omitted from the list.
<!-- id: REQ-DCLEAN-019.A3 -->
- An expanded category with no items shows the one-line note "No items".

<!-- id: REQ-DCLEAN-020 -->
## The component MUST be fully operable by keyboard in a predictable focus order

implementations:
  - gui/src/screens/SmartClean.tsx:SmartClean
  - gui/src/screens/smartClean/ConfirmClean.tsx:ConfirmClean
  - gui/src/screens/smartClean/Results.tsx:Results

verifies:
  - gui/tests/visual/keyboard.spec.ts:fullFlowByKeyboardOnly
  - gui/src/screens/__tests__/controls.test.tsx:focusMovesToEachPhaseHeading
  - gui/src/screens/__tests__/controls.test.tsx:firstRenderLeavesFocusAlone
  - gui/src/screens/__tests__/SmartClean.test.tsx:modeSwitchIsARadiogroupWithArrowKeys

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

implementations:
  - gui/src/screens/smartClean/Results.tsx:CategoryRow
  - gui/src/screens/smartClean/resultsCopy.ts:disclosureLabel
  - gui/src/screens/smartClean/Scanning.tsx:Scanning
  - gui/src/screens/smartClean/Cleaning.tsx:Cleaning
  - gui/src/shell/UsageRing.tsx:UsageRing
  - gui/src/shell/DiskMeter.tsx:DiskMeter

verifies:
  - gui/src/screens/__tests__/controls.test.tsx:checkboxIsNamedByCategoryAndDescribedBySafety
  - gui/src/screens/__tests__/controls.test.tsx:disclosureExposesStateAndRegion
  - gui/src/screens/__tests__/controls.test.tsx:progressBarsExposeValueAndRange
  - gui/src/screens/__tests__/controls.test.tsx:meterAndRingHaveTextEquivalents
  - gui/tests/visual/a11y.spec.ts:everyPhasePassesAxe

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

implementations:
  - gui/src/screens/smartClean/ConfirmClean.tsx:ConfirmClean

verifies:
  - gui/src/screens/__tests__/SmartClean.test.tsx:dialogIsModalAndLabelledByItsTitle
  - gui/src/screens/__tests__/SmartClean.test.tsx:focusOpensOnCancel
  - gui/src/screens/__tests__/SmartClean.test.tsx:focusIsTrappedInsideTheDialog
  - gui/src/screens/__tests__/SmartClean.test.tsx:focusReturnsToTheOpener
  - gui/src/screens/__tests__/SmartClean.test.tsx:focusFallsBackToTheResultsHeadingWithoutAnOpener
  - gui/tests/visual/shell.spec.ts:confirmDialogMatchesSnapshot

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

implementations:
  - gui/src/screens/smartClean/announce.ts:announcement
  - gui/src/screens/SmartClean.tsx:SmartClean
  - gui/src/screens/smartClean/GoalNotification.tsx:GoalNotification

verifies:
  - gui/src/screens/__tests__/announce.test.ts:scanAnnouncesStartAndEachFinishedCategoryOnly
  - gui/src/screens/__tests__/announce.test.ts:completionAnnouncesThePhaseHeading
  - gui/src/screens/__tests__/announce.test.ts:cleanAnnouncesStartAndEachCategory
  - gui/src/screens/__tests__/announce.test.ts:selectionAndDialogChangesAreSilent
  - gui/src/screens/__tests__/controls.test.tsx:announcesScanStartCategoriesAndTotal
  - gui/src/screens/__tests__/controls.test.tsx:announcesCleanStartAndFreedTotal
  - gui/src/screens/__tests__/controls.test.tsx:errorsAreAnnouncedInAnAlert
  - gui/src/screens/__tests__/SmartClean.test.tsx:notificationShowsOnIdleBelowGoal
  - gui/tests/visual/a11y.spec.ts:statusIsAnnouncedPolitelyWithoutStealingFocus
  - gui/tests/visual/a11y.spec.ts:everyPhasePassesAxe

**Acceptance**
<!-- id: REQ-DCLEAN-023.A1 -->
- Scan and clean progress is announced politely at coarse intervals (for example, per category completed), not on every tick.
<!-- id: REQ-DCLEAN-023.A2 -->
- Phase completion ("Found <total> you can clear", "<freed> cleared") is announced.
<!-- id: REQ-DCLEAN-023.A3 -->
- The below-goal notification is announced via polite status semantics.

<!-- id: REQ-DCLEAN-024 -->
## Text and interface elements MUST meet WCAG 2.2 AA contrast using tokens only

implementations:
  - gui/src/lib/contrast.ts:parseTokens
  - gui/src/lib/contrast.ts:resolveColor
  - gui/src/lib/contrast.ts:contrastRatio
  - gui/src/lib/contrast.ts:pairRatio

verifies:
  - gui/src/styles/contrast.test.ts:textPairsMeetAA
  - gui/src/styles/contrast.test.ts:uiPairsMeetAA
  - gui/src/styles/contrast.test.ts:bodyAndSecondaryTextOnBothGrounds
  - gui/src/styles/contrast.test.ts:accentTextUsesADeepStep
  - gui/src/styles/contrast.test.ts:mutedTextIsUnused
  - gui/src/styles/contrast.test.ts:newColourTokensAliasOrganicSteps
  - gui/src/styles/contrast.test.ts:everyForegroundInUseIsMeasured
  - gui/tests/visual/a11y.spec.ts:everyPhasePassesAxe

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

implementations:
  - gui/src/shell/UsageRing.tsx:UsageRing
  - gui/src/shell/DiskMeter.tsx:DiskMeter
  - gui/src/screens/smartClean/Scanning.tsx:Scanning
  - gui/src/screens/smartClean/Results.tsx:CategoryRow

verifies:
  - gui/tests/visual/motion.spec.ts:transitionsUseTheMotionTokens
  - gui/tests/visual/motion.spec.ts:reducedMotionDisablesTransitionsAndAnimations
  - gui/src/screens/__tests__/SmartClean.test.tsx:chevronRotatesWithTheMotionToken

**Acceptance**
<!-- id: REQ-DCLEAN-025.A1 -->
- Ring, bar and chevron transitions use `--motion-ring`, `--motion-bar` and `--motion-chevron` respectively.
<!-- id: REQ-DCLEAN-025.A2 -->
- With reduced motion requested, these transitions are disabled and state changes apply immediately.

<!-- id: REQ-DCLEAN-026 -->
## The window MUST open at the design size and MUST NOT shrink below it

The snapshot is a fixed 1200 × 780 window with no breakpoints, so the GUI is a fixed-layout desktop window.

implementations:
  - gui/src/shell/AppShell.tsx:AppShell
  - gui/src/screens/smartClean/Results.tsx:Results

verifies:
  - gui/src/shell/__tests__/window.test.ts:windowOpensAtTheDesignSize
  - gui/src/shell/__tests__/window.test.ts:windowMinimumIsTheDesignSize
  - gui/tests/visual/window.spec.ts:extraSpaceGoesToTheMainArea
  - gui/tests/visual/window.spec.ts:resultsListScrollsIndependently

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

implementations:
  - src/contract.rs:CleanupView
  - src/contract.rs:CategoryView
  - src/contract.rs:ItemView
  - src/prefs.rs:Prefs
  - src/contract.rs:ScanProgress
  - src/contract.rs:CleanResult
  - src/contract.rs:categories_from_hits
  - src/contract.rs:cleanup_view
  - src/contract.rs:scan_progress
  - src/contract.rs:scan_finished
  - src/contract.rs:load
  - src/disk.rs:Disk
  - src/disk.rs:disk_usage
  - src/cleanup.rs:Category
  - src/cleanup.rs:CATEGORIES
  - src/cleanup.rs:Safety
  - src/cleanup.rs:CleanupHit
  - src/schema.rs:cleanup_categories
  - gui/src/lib/contract.ts:parseCleanupView
  - gui/src/lib/contract.ts:parseScanProgress
  - gui/src/lib/contract.ts:parseCleanResult
  - gui/src/lib/contract.ts:ContractError
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - src/contract.rs:category_bytes_equal_item_sum
  - src/contract.rs:item_name_is_basename
  - src/contract.rs:category_ids_come_from_engine_table
  - src/contract.rs:contract_round_trips_and_rejects_unknown_safety
  - src/contract.rs:default_preferences_are_all_on
  - src/contract.rs:scan_progress_fraction_is_clamped
  - src/disk.rs:disk_usage_reports_physical_volume_space
  - src/disk.rs:disk_usage_fails_for_missing_path
  - src/disk.rs:mount_point_of_root_is_root
  - src/schema.rs:cleanup_categories_carry_label_and_description
  - tests/lib_integration.rs:contract_totals_match_cleanup_summary
  - tests/lib_integration.rs:contract_load_builds_view_from_snapshot
  - gui/src/lib/contract.test.ts:validViewPasses
  - gui/src/lib/contract.test.ts:rejectsUnknownSafety
  - gui/src/lib/contract.test.ts:rejectsFractionalBytes
  - gui/src/lib/contract.test.ts:rejectsStringBytes
  - gui/src/lib/contract.test.ts:rejectsCategoryBytesThatAreNotTheItemSum
  - gui/src/lib/contract.test.ts:progressFractionMustBeInZeroToOne
  - gui/src/lib/contract.test.ts:cleanResultParsesWithAndWithoutEntry
  - gui/src/screens/__tests__/SmartClean.test.tsx:rendersExactlyTheEngineCategories
  - gui/src/screens/__tests__/SmartClean.test.tsx:emptyViewShowsNothingToClean

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

implementations:
  - gui/src/lib/format.ts:formatBytes
  - gui/src/lib/format.ts:NUMERIC_CLASS
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - gui/src/lib/format.test.ts:formatsBoundaries
  - gui/src/lib/format.test.ts:usesDecimalBase
  - gui/src/lib/format.test.ts:numericClassUsesTabularFigures
  - gui/src/shell/__tests__/meter.test.ts:formatBytesFollowsDecimalRules

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

implementations:
  - src/contract.rs:clean
  - src/contract.rs:select_reviewed
  - src/contract.rs:is_stale
  - src/contract.rs:cleanup_view
  - src/contract.rs:view_hits_with
  - src/cleanup.rs:apply_with
  - src/cleanup.rs:Failure
  - src/cleanup.rs:in_scope
  - gui/src/lib/contract.ts:parseCleanupView
  - gui/src/screens/SmartClean.tsx:SmartClean

verifies:
  - src/contract.rs:clean_reports_missing_and_proceeds
  - src/contract.rs:view_marks_snapshot_stale_after_24h
  - tests/lib_integration.rs:contract_never_lists_ignored_or_excluded_paths
  - gui/src/lib/contract.test.ts:rejectsNegativeBytes
  - gui/src/lib/contract.test.ts:rejectsMissingFieldsWithoutAPartialList
  - gui/src/screens/__tests__/SmartClean.test.tsx:malformedDataShowsTheErrorStateNotAPartialList
  - gui/src/screens/__tests__/SmartClean.test.tsx:staleSnapshotWarnsBeforeCleaning

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

implementations:
  - gui/src/lib/budgets.ts:IDLE_INTERACTIVE_BUDGET_MS
  - gui/src/lib/budgets.ts:INTERACTION_BUDGET_MS
  - gui/src/lib/budgets.ts:RESULTS_ITEM_CAP
  - gui/src/lib/budgets.ts:IDLE_INTERACTIVE_MARK
  - gui/src/screens/smartClean/resultsCopy.ts:visibleItems
  - gui/src/screens/smartClean/resultsCopy.ts:moreItemsLabel
  - gui/src/screens/smartClean/Idle.tsx:Idle
  - gui/src/screens/smartClean/Results.tsx:CategoryRow
  - src/cleanup.rs:scan_with

verifies:
  - gui/tests/visual/perf.spec.ts:idleIsInteractiveWithinBudget
  - gui/tests/visual/perf.spec.ts:interactionsRespondWithinBudget
  - gui/src/screens/__tests__/controls.test.tsx:capsItemsAtTheLargest200WithMore
  - gui/src/screens/__tests__/resultsCopy.test.ts:visibleItemsKeepsAllUpToTheCap
  - gui/src/screens/__tests__/resultsCopy.test.ts:visibleItemsShowsTheLargestBeyondTheCap
  - tests/lib_integration.rs:cleanup_is_fast_on_large_snapshot

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

implementations:
  - gui/scripts/lint-tokens.mjs:stripComments
  - gui/src/lib/contrast.ts:parseTokens

verifies:
  - gui/src/styles/lints.test.ts:lintTokensFindsNoLiterals
  - gui/src/screens/__tests__/SmartClean.test.tsx:dangerAndBackdropUseTheirTokens
  - gui/src/styles/contrast.test.ts:newColourTokensAliasOrganicSteps

**Acceptance**
<!-- id: REQ-DCLEAN-031.A1 -->
- Component code contains no literal colour, font family, pixel size or duration that a token in `tokens.css` already carries.
<!-- id: REQ-DCLEAN-031.A2 -->
- Headings use `--font-heading` and body text uses `--font-body`; screen titles use `--fs-screen-title` / `--fs-screen-hero`, and the ring figure uses `--fs-ring-number`.
<!-- id: REQ-DCLEAN-031.A3 -->
- Elevation uses the `--shadow-*` tokens exactly where the snapshot uses them (window, dialog, notification, cards), and nowhere else.
<!-- id: REQ-DCLEAN-031.A4 -->
- New tokens live only in `gui-tokens.css`, never in the vendored `tokens.css` or `organic.css`, and alias an Organic token wherever one exists (`--color-danger` is `--color-accent-800`). They are defined before the components that use them.

<!-- id: REQ-DCLEAN-032 -->
## The app shell MUST reproduce the snapshot's window chrome, sidebar and screen frame

The shell is the frame around every screen: a title bar of `--layout-titlebar-h`, a sidebar of `--layout-sidebar-w` with the brand, seven navigation entries (Smart Clean, Assistant, Space Map, Large files, Duplicates, History, Settings), the assistant badge, and the disk-usage meter (REQ-DCLEAN-002).

implementations:
  - gui/src/shell/AppShell.tsx:AppShell
  - gui/src/shell/TitleBar.tsx:TitleBar
  - gui/src/shell/Sidebar.tsx:Sidebar
  - gui/src/shell/Brand.tsx:Brand
  - gui/src/shell/nav.ts:NAV_ITEMS
  - gui/src/shell/NavItem.tsx:NavItem
  - gui/src/shell/DiskMeter.tsx:DiskMeter
  - gui/src/lib/meter.ts:goalPos
  - gui/src/App.tsx:ROUTES
  - gui/src-tauri/src/main.rs:main

verifies:
  - gui/src/shell/__tests__/nav.test.ts:navHasSevenEntriesInSnapshotOrder
  - gui/src/shell/__tests__/nav.test.ts:onlyAssistantCarriesBadge
  - gui/src/shell/__tests__/nav.test.ts:navRoutesAreUniqueAndSmartCleanIsHome
  - gui/src/shell/__tests__/Shell.test.tsx:brandAndTitleBarReadDisky
  - gui/src/shell/__tests__/Shell.test.tsx:windowTitleIsDisky
  - gui/src/shell/__tests__/Shell.test.tsx:everyNavRouteRendersARealScreen
  - gui/src/shell/__tests__/Shell.test.tsx:sidebarShowsSevenEntriesAndAssistantBadge
  - gui/src/shell/__tests__/Shell.test.tsx:meterShowsFixtureFreeSpaceAndGoal
  - gui/src/shell/__tests__/meter.test.ts:goalPosMatchesSnapshotFixture
  - gui/src/shell/__tests__/meter.test.ts:goalPosClampsToTheBar
  - gui/src/shell/__tests__/meter.test.ts:goalPosIsZeroWithoutCapacity
  - gui/tests/visual/shell.spec.ts:snapshotBaselineRenders
  - gui/tests/visual/shell.spec.ts:titleBarMatchesSnapshot
  - gui/tests/visual/shell.spec.ts:sidebarMatchesSnapshot
  - gui/tests/visual/shell.spec.ts:trafficLightSlotSitsWhereSnapshotDrawsDots
  - gui/tests/visual/shell.spec.ts:everyScreenKeepsTheShell
  - gui/tests/visual/shell.spec.ts:rendersOfflineWithBundledFontsAndIcons

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
  - src/cleanup.rs:outermost
  - src/cleanup.rs:root_sums
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
  - src/cleanup.rs:drop_nested_sees_through_a_sibling_with_a_space
  - tests/lib_integration.rs:nested_dev_folders_sum_once_under_the_outermost
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
  - Nested matches are pruned to the outermost before anything is summed, and every file counts once, under its outermost match. A match nested inside a recently used one is never reported on its own.
  - Sums come from linear passes, with no join or sort over every path, so a 10 M-row home stays in seconds, not tens of seconds.
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

