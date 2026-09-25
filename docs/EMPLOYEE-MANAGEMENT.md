# Employee Management (development)

The Rust client uses Main POS's existing PostgreSQL employee tables. It does not
copy employee data into a separate database. This feature has not been released.

## Available workflows

- Employees: create/edit, photo, POS account link, contact/emergency details,
  employment status, branch/position/department filters.
- Attendance: date range, manual entries and corrections with required reason.
- Shifts: create/edit, assign/edit/remove assignments, attendance reclassification
  preserving manually corrected records.
- Payroll: draft with allowances/deductions, payment and linked Salaries expense
  committed together.
- Leave: request, approve, reject or cancel; attendance reclassification.
- Documents: metadata, file path, expiry summary.
- Advances and commission: advance entry, bounded repayments, commission rules.
- Performance: employee sales/refunds/discounts and commission calculation.
- Cash sessions: open/close, expected versus actual cash.
- All sections: search, paginated display, Excel export of filtered rows.

Named Main POS permissions are rechecked on every read/write. The current Rust
sidebar exposes Employees to Admin/Manager; arbitrary custom-role login and
cashier employee access are not implemented. Viewing a tab does not grant access.

## Deployment prerequisite

Back up the database. Initialize/upgrade Employee Management using Main POS first.
As database owner, apply `migrations/003_employee_requests.sql` once to the shared
database. Grant the existing client role SELECT and INSERT on that ledger; keep
existing HR table/sequence privileges consistent with the deployment. The client
does not grant itself permissions or automatically migrate production data.

The migration has only been applied to an isolated local test database, not the
user's live database. Main POS and Rust should be tested together on a restored
copy before client rollout.

## Safety and recovery

Writes use transactions, record revisions, freshly checked permissions and a
request ledger. Payroll payment and repayment retries reuse their request ID.
Before submission, the client persists a recovery file under the current Windows
user's local application data, `KAY POS Rust/employee-pending`. This contains the
pending form data, potentially sensitive employee details, but no DB password.
It is removed after confirmed completion. Restrict access to the Windows account.

After interruption, reopen Employees as the original operator and retry the same
request. Do not delete a recovery file merely to bypass an unknown save result.
If recovery fails because access/schema changed, an administrator must restore
access and reconcile the ledger before clearing it.

## Parity limits / not yet verified

- ZKTeco device configuration and attendance sync are now available; see
  `ZKTECO-DEVICES.md`. Employee/device-user mappings are still managed in Main POS.
  No live device was contacted during implementation.
- Document file paths follow Main POS behavior; files are not uploaded/shared
  automatically between PCs. Use an appropriately secured shared path if needed.
- Payroll advance deductions do not automatically repay salary advances, matching
  Main POS. Record repayment separately. Cash-session calculations match Main POS:
  opening cash plus completed cash sales by the linked POS user since opening;
  expenses/refunds are not separately deducted.
- Native window interaction and visual layout still need an acceptance run.
  No production HR records were created/edited during implementation.

## Verification

`cargo check -p pos_desktop --locked`

`cargo test -p pos_core -p pos_desktop --lib --bins --locked`

The ignored integration test `tests/employees.rs` requires an isolated PostgreSQL
instance at 127.0.0.1:55487, via EMPLOYEE_TEST_DATABASE_URL. It creates its own schema
and exercises create/edit, stale-revision protection, role permissions, shifts,
attendance, leave, payroll concurrent retries, overpayment rejection, repayment
replay, documents, commission, cash sessions and all section queries. Never point
it at a production database.
