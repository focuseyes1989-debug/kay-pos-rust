# KAY POS Rust 0.2.10

## Changes
- POS-only default: ERP Operations is removed from navigation.
- ERP delivery, master-data and reconciliation workers do not start in this build.
- Sale, refund, expense, payment and procurement operations skip ERP hooks, avoiding ERP table dependencies during normal POS work.
- Existing POS functions, AI chat and receipt dialogs remain available. Existing ERP records are retained.

## Installation
Finish sales and resolve pending checkout requests, sign out, then use Check for Updates → Download & Install / Restart. The signed ZIP contains only pos_desktop.exe; preserve database configuration and pending files.

Uses the same signing key as v0.2.9. Clients with older incompatible keys require manual executable replacement after closing the app; never bypass signature checks. Microsoft Edge WebView2 Runtime is required.

## Server note
The configured shop server already has migration 015 applied after a verified database backup. It suppresses new ERP outbox entries from inventory triggers and older clients without deleting previous events or changing POS sales/stock/balances. This release does not apply migrations automatically. Other servers that previously enabled ERP must apply migrations/015_erp_disabled.sql separately to stop database-triggered queue entries. Servers without ERP tables do not need ERP migrations for this POS-only build.

Re-enabling ERP requires a deliberate integration build and server configuration change. Historic pending entries are preserved and are not automatically delivered by this build.

## Verification
Core and desktop unit tests, default-build ERP worker checks, executable version self-check, signed ZIP verification, tamper rejection and staged updater tests. Production database data is not modified during release packaging.
