# KAY POS 0.2.31

- Add Serial & Warranty under Sales for solar panels, inverters, batteries and other serialized products.
- Register sold serial numbers against completed receipts with a customer; warranty starts on the original sale date.
- Search by serial, receipt, customer or product. Track claim issues, coverage, status and service history.
- Prevent duplicate serials, excess registrations and duplicate active claims. Check expiry and refunded receipts.
- Managers/admins can process, resolve or reject claims with recorded notes and actor history.
- Support PostgreSQL and SQLite, including manual export/import of serials, warranties and claims.

Select a customer during checkout, then register each sold unit from Sales > Serial & Warranty. Warranty registration is performed after checkout. Claims do not automatically change stock or payments.

PostgreSQL creates additive extension tables on first module use; the database account needs schema creation permission for that initial setup. Existing sales and product data are preserved. Older standalone files gain empty extension tables when opened.

Validation: 134 core tests and 75 desktop tests passed, including isolated PostgreSQL, SQLite, existing POS regressions and warranty export/import.
