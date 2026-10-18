# KAY POS 0.2.42

- Show only parent locations in the Cart selling-location selector. Parent selections continue to include all descendant stock.
- Normalize an existing child selection to its parent and retain the saved parent default.
- Fix location changes such as Shop2 being reset by an older catalog snapshot.
- Fix Sales remaining on Loading by subscribing to catalog completion while the resource is pending.
- Preserve the selected location across catalog refreshes and handle rapid location changes and load errors.

Validated with initial-load, selection-change, cancellation and error regression tests, location hierarchy tests, desktop/core/standalone tests and light/dark dropdown checks.
