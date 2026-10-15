# KAY POS 0.2.36

- Export Inventory > Stock value to Excel with separate Stock value and Products without cost sheets. Include all rows matching the current search/category, numeric stock and cost cells, totals, export time, filters and frozen headings.
- Replace the Sales category sidebar with a searchable category combo box beside product search, freeing space for products.
- Keep search, category selection and item count on one compact header row. Remove the Products title and category icon; reserve item-count width so filtering does not shift controls.
- Add Top 7 category quick buttons plus All, ranked by sales amount over the last 30 calendar days. Returned quantities and non-completed receipts are excluded; ranking is before invoice-level discounts.
- Keep quick buttons and category selection in sync.
- Fix enhanced dropdowns appearing on sign-in, constrain popup height with scrolling, position above when needed, and close reliably on selection, Escape or outside clicks.

Validated with Excel export checks, category-ranking integration tests, desktop/core tests and light/dark dropdown layout checks.