# Feature file for disky's agentic disk-management surface.
#
# Each scenario doubles as documentation AND acceptance criteria.
# When a scenario is implemented, the matching test lives in
# `tests/bdd/<slug>.rs` and drives the lib API to verify
# Given/When/Then literally. Gherkin syntax (Cucumber compatible) —
# no harness required to parse; we keep it human-readable.

Feature: Detect directory growth between snapshots
  As a disk-conscious agent
  I want to know which directories grew since the last scan
  So that I can flag the source of disk pressure precisely

  Background:
    Given a snapshot A taken at 2026-05-15T00:00:00Z
    And a snapshot B taken at 2026-05-22T00:00:00Z

  Scenario: directory grew between two snapshots
    Given /Users/me/Library/Caches measured 4 GB in A
    And /Users/me/Library/Caches measured 6 GB in B
    When I run `disky growth --since @latest~1 --limit 10`
    Then the output kind is "growth"
    And the records include /Users/me/Library/Caches with delta_bytes = 2147483648
    And the record carries rate_bytes_per_day computed from the snapshot delta

  Scenario: directory shrank counts as negative growth
    Given /Users/me/Downloads measured 12 GB in A
    And /Users/me/Downloads measured 3 GB in B
    When I run `disky growth --since @latest~1`
    Then /Users/me/Downloads appears with negative delta_bytes
    And rate_bytes_per_day < 0

  Scenario: directory not present in B counts as removed
    Given /tmp/big-cache exists only in snapshot A
    When I run `disky growth --since @latest~1`
    Then /tmp/big-cache appears with kind = "removed"
    And delta_bytes equals the negation of its size in A

Feature: Classify growth pattern across multiple snapshots
  As an agent advising on cleanup
  I want each growing directory tagged with a pattern
  So that I can distinguish chronic log accumulation from one-off bursts

  Scenario: linear log accumulation is classified as log-shaped
    Given 7 daily snapshots
    And /var/log grew by between 95 MB and 110 MB in every interval
    When I run `disky pattern --over 7d`
    Then /var/log appears with kind = "log-shaped"
    And confidence > 0.9

  Scenario: sudden jump is classified as burst
    Given 7 daily snapshots
    And /Library/Containers/com.docker.docker stayed flat for 6 intervals then grew 5 GB in the 7th
    When I run `disky pattern --over 7d`
    Then the docker container directory appears with kind = "burst"

  Scenario: stable directory is excluded by default
    Given /System measured 8 GB in every snapshot
    When I run `disky pattern --over 7d`
    Then /System does not appear in the records
    But with `--include-stable` it appears with kind = "stable"

Feature: Predict when disk fills based on observed growth
  As an agent that warns users early
  I want a fill-by-date prediction for the current volume
  So that cleanup recommendations have a deadline

  Scenario: linear extrapolation produces a fill date
    Given the volume has 30 GB free
    And the total used-bytes grew by 1 GB/day over the last 14 snapshots
    When I run `disky predict`
    Then the output kind is "predict"
    And the record carries fill_at as an RFC 3339 timestamp ~30 days out
    And the record carries confidence_lower and confidence_upper bounds

  Scenario: insufficient history returns null prediction with explanation
    Given fewer than 3 snapshots exist
    When I run `disky predict`
    Then fill_at is null
    And reason contains "insufficient history"

Feature: Find log generators (chronic small-write directories)
  As an agent identifying noisy processes
  I want directories with many small files that change frequently
  So I can suggest log rotation or archival

  Scenario: high file count + frequent mtime updates marks a log generator
    Given a snapshot where /var/log has 5000 files
    And 80% of those files have mtime within the last 24h
    And mean file size is < 100 KB
    When I run `disky churn --over 24h`
    Then /var/log appears with kind = "log-generator"
    And churn_score > 0.8
