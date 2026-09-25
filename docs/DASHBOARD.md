# Dashboard

Reports > Dashboard is available to managers and administrators. Core queries reauthorize the session and use a read-only repeatable-read transaction. Inventory alerts are fetched separately after the financial snapshot. Header Refresh reloads the page without modifying records.

Period metrics use inclusive local calendar dates. Sales received is payment minus change, not total sales; credit collections are separate receipt events. Refunds are grouped by original sale date, not refund payment date. Expenses and supplier payments are separate source totals and must not be summed blindly: Main POS may record the same disbursement in both. No cash-on-hand or audited net-profit claim is made.

Current customer balances use customers.current_balance, including prior balances/adjustments. Outstanding invoice balances come from credit_sales and may differ from customer totals. Supplier balances use the Main POS supplier_payments ledger: Purchase adds debt, other non-null types reduce it. Advances are displayed separately, not netted against other suppliers. Purchase orders without ledger entries are highlighted, not presumed unpaid or double-counted. HR active headcount is shown when its table exists. Employee workflows remain accessible through the Employees link.

Credit marked fully paid is an attention metric, not proof of incorrect data: legitimate fully paid credit-labelled sales are possible. The dashboard never automatically repairs balances.

No migrations or production writes. Existing shared Main POS tables are required; query failures display an error rather than zero balances. Stock alerts include variants without counting their parent product again. Long tables scroll rather than silently truncating totals.

Verification: isolated PostgreSQL test `dashboard_separates_credit_collections_and_current_balances`; layout harness `scripts/test-dashboard-layout.cjs`.
