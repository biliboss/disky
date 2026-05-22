# Composability scenarios — how agents chain disky calls.
#
# Driven by the `--json-input` flag (M3 in the plan). Lets one disky
# command consume the envelope of another, no DuckDB round-trip needed.

Feature: Pipe top output through filter
  Scenario: filter top files by size threshold
    Given a snapshot containing files of varied sizes
    When I run `disky top --format json | disky filter --where "size > 1GB"`
    Then the filter output kind is "filter"
    And every record has size > 1073741824
    And schema_version is 1

  Scenario: chain growth and filter for "things bigger than 100MB that grew this week"
    When I run `disky growth --over 7d --format json | disky filter --where "delta_bytes > 100MB"`
    Then the records satisfy both conditions

Feature: Re-aggregate without re-scanning
  Scenario: re-extract dirs from a top envelope
    Given a captured `disky top --limit 1000 --format json` envelope on stdin
    When I run `disky dirs --json-input` on that input
    Then dirs are computed from the records, no snapshot is opened
    And the snapshot field is absent in the output

Feature: Mutual exclusion of --json-input and --snapshot
  Scenario: passing both flags exits with usage error
    When I run `disky top --json-input --snapshot @latest`
    Then exit code is 2
    And stderr carries RFC 9457 problem details with type containing "/errors/usage"
