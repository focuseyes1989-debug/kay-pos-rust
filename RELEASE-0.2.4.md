# KAY POS Rust 0.2.4

Windows x64 signed update.

## Important: One-Time Manual Installation Required
The previous update signing key was lost. Version 0.2.4 starts a new signing-key baseline. Versions 0.2.0 through 0.2.3 cannot authenticate this update and must NOT bypass signature verification.

Download the ZIP from this official release page. Finish all sales, resolve pending checkouts, and close KAY POS on the client. Keep a backup of the existing EXE, then extract and replace only `pos_desktop.exe` in the existing client folder. Keep database configuration and pending-data files unchanged. Reopen and confirm version 0.2.4. Pilot on one client first. Future compatible releases signed with the new key can use the built-in updater.

## Changes
- Employee Management: employees, attendance, shifts/assignments, payroll, leave, documents, advances/commission, performance and cash sessions using Main POS's shared database.
- Receipt-style employee tabs, aligned filters and project-consistent buttons.
- Attendance date shortcuts: Today, This week (Monday through today), This month.
- Settings > ZKTeco Device: device configuration and TCP connection probe.
- Attendance Sync from Settings and Attendance: saved device authentication, TCP-to-UDP fallback, duplicate prevention, atomic imports, manual-correction preservation and clearer connection errors. Device logs are never cleared.
- Header-only page Refresh; Sign out moved to the side menu.
- Sales catalog/variant stock reload when returning from inventory, with a refresh action that preserves cart quantities and prices.
- Compact product cards and locally saved theme applied before asynchronous settings load.

## Before Rollout
Back up the shared database and pilot on one client first. Existing Main POS Employee tables and employee/device-user mappings are required. Initialize Employee Management and ZKTeco in Main POS first; the Rust client does not automatically migrate production tables.

Employee writes additionally require this one-time owner-run migration:

```sql
CREATE TABLE IF NOT EXISTS rust_employee_requests (
    request_id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    operation TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    record_id INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

The database administrator must grant SELECT and INSERT on this ledger to the existing client role, and retain the appropriate HR/ZKTeco table and sequence privileges. Do not grant deletion of completed request records. Configure device-user/employee mappings in Main POS. No shop data, credentials or private signing keys are included in the update.

## Known Limits
- K20 hardware sync has not been validated against a live device. A listed device only confirms a saved database record; TCP probe success does not validate Comm Key or attendance import. Test sync on the actual LAN before wider rollout.
- Device mapping management remains in Main POS. Documents retain shared/local file-path behavior.
- Payroll advance deductions and cash-session calculations retain Main POS behavior; payroll deductions do not automatically repay salary advances.

## Install
For this release, follow the one-time manual installation above; the old built-in updater will reject the new signature. Microsoft Edge WebView2 Runtime is required. The ZIP signature is not an Authenticode certificate; Windows SmartScreen may still warn.

## Verification
Core/desktop unit tests, isolated PostgreSQL employee/device/sync tests, and browser component/layout checks were run. No live attendance device or production HR records were modified during verification.

Attendance transport uses the MIT-licensed rustzk 1.1.0 connector: https://github.com/vkaylee/rustzk.
