# KAY POS 0.2.3

- Inventory Adjustment now sets the remaining quantity at the selected location, not the total across locations. Shows location stock and exposes legacy/unassigned variant balances.
- Adjustment and Stock Out require Manager/Admin authorization and record the signed-in operator.
- View Movements has a wider responsive layout, readable dates, signed quantities, expandable notes and variant batch details.
- Reverse Stock In with a reason and confirmation. Checks original batch availability, prevents repeated reversal, and records an audit trail. Missing or ambiguous original batch allocations require a reviewed adjustment instead.
- Stock reversal does not reverse cost calculations or supplier payments.
- Product image loading follows product/database identity and rejects stale image responses.

Signed Windows x64 package containing only pos_desktop.exe. No schema migration or database configuration is included.

Finish work and sign out before updating. On the login screen check for updates, download/install, then restart. Pilot on one client and verify a stock adjustment before updating the remaining clients.

Validation: automated workspace unit tests, keyboard shortcut tests, signed package verification and staged updater tests. Database integration tests require an isolated test database and were not run. Live inventory changes and native UI visual checks were not exercised for this release.
