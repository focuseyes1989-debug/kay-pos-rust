# POS-only mode

The default Rust build disables ERP integration. ERP Operations is hidden; delivery, master-data and reconciliation workers do not start even if ERP environment variables remain configured. Sale/refund, expense, customer/supplier payment and procurement ERP hooks return before querying ERP tables. Existing ERP modules and records remain available for a future integration build using the explicit `pos_core/erp-integration` feature.

Migration 015 disables creation of new ERP outbox records on the server, including inventory trigger events and events from older clients. The guard only affects INSERT on rust_erp_outbox. It preserves all existing ERP events and does not modify POS sales, stock movements, payments or balances. Older binaries may still create ERP accounting documents until replaced with the POS-only build, but those documents are not queued for delivery.

Before applying migration 015, take and verify a database backup. The configured server was backed up on 2026-10-05 before application. All 22 existing outbox records were retained and a synthetic queue insert was verified to produce no new record. Default-build worker tests and core unit tests verify the source gates.

Re-enabling ERP requires both an explicit integration build and an intentional server change to rust_erp_runtime.enabled=true, after checking endpoint configuration and reviewing retained pending events. Do not automatically resend historic events.

The v0.2.9 binary predates this POS-only source change. Release v0.2.10 includes the hidden ERP menu, disabled workers and skipped client-side ERP hooks. Server queue suppression takes effect immediately; client-side changes take effect after installing v0.2.10.
