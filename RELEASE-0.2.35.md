# KAY POS 0.2.35

- Refresh cards throughout the app with compact 8px corners, subtle borders and shadows, and coloured icons on summary card headings.
- Add matching icons to tabs in Receipts, Expenses, Inventory, Locations, Reports, Purchases, Sale Summary, Employees and Service Orders. Keep selected-tab icons readable and long tab strips scrollable.
- Show current stock capital on Dashboard with a Stock value card.
- Add Inventory > Stock value with product/variant stock, unit cost, calculated value, search/category filtering and a separate list of products without cost.
- Exclude Service products and zero/missing costs from valuation. Count variant stock and costs once, without adding parent stock or location allocations again.
- Flag negative stock balances and exclude their quantities from stock value. Handle legacy empty numeric product fields safely.

Stock value reflects current positive on-hand stock at current recorded cost, not historical purchase totals. Missing costs are shown separately so incomplete valuations can be identified.

Validated with 80 core tests, 20 PostgreSQL/SQLite integration tests, desktop tests, and light/dark card and tab layout checks.