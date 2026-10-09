# Stock transfer

Locations > Stock Transfer uses the Python POS PostgreSQL tables without a migration.

Select a product (name/barcode/SKU search), variant where applicable, source and destination location, quantity, reason and optional notes. Source availability can be refreshed. Product information and image are shown alongside the form. A successful transfer refreshes Stock by location; variants are included in that list.

The core operation requires a current Manager/Admin session, locks stock rows, verifies the displayed source balance, and moves stock within one serializable transaction. Total product/variant stock remains unchanged. Batch numbers, expiry dates and variant expiry-unknown flags are retained. Standard products use paired out/in stock_movements with one TRF reference, matching the Python service. Variants use a transfer movement and balanced variant_batch_changes. Legacy variant stock is materialized as Legacy / unassigned, following existing Rust/Python batch handling. Duplicate references, same locations, nonpositive quantity, insufficient stock and stale snapshots are rejected. Errors roll back all changes, including audit records.

Verification: core validation tests and an opt-in PostgreSQL integration test using exclusively session-local TEMP tables (one connection). Set KAY_TRANSFER_TEST_CONFIG to a local database config JSON file and run cargo test -p pos_core --lib stock_transfer:: -- --include-ignored. The test does not read or modify shop stock; TEMP tables shadow every application table used by the operation and disappear when the connection closes.
