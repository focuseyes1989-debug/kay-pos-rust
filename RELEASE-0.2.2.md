# KAY POS 0.2.2

- Repair stale parent stock totals during variant checkout within the sale transaction.
- Match Main POS legacy variant batch reconciliation: record unallocated stock as LEGACY-OPENING at Legacy / unassigned, with unknown expiry. Preserve existing batches; reject overallocated or invalid stock.
- Apply Regional currency selection throughout monetary displays and receipt printing.
- Unified update action with download progress and verification/install states.

Signed Windows x64 update. No schema migration is included. The package contains only pos_desktop.exe, not database configuration or shop data.

Pilot on one client first. Normal update: sign out, check for updates on the login screen, download/install, then restart.

For a client blocked by unresolved checkout, close KAY POS completely and manually replace only pos_desktop.exe from the signed release ZIP in the existing client folder. Preserve the database configuration and pending checkout data. Reopen and use Retry original sale; do not create a replacement sale or delete recovery files. If batch totals exceed stock, review the stock discrepancy instead of bypassing the guard.

Validation: core and desktop automated unit tests; signed-package verification and staged updater tests. Database integration tests require an isolated test database and were not run for this release. Physical printer and live client checkout were not exercised.
