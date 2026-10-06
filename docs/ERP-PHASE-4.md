# ERP Phase 4: Finance and Accounting

Phase 4 creates balanced double-entry documents in the same PostgreSQL transaction as each POS business operation. Every document is also queued through the Phase 1 outbox as `accounting.journal.created`.

## Covered Transactions

- Completed cash, card, mobile and credit sales
- Sales discounts, tax payable, cost of goods sold and inventory relief
- Full sale refunds through exact reversal of the original journal
- Customer credit collections and collection refunds
- Goods receipts as Inventory / Accounts Payable
- Supplier payments as Accounts Payable / Payment Account
- Expense create, revision and deletion reversal

Each line contains an account mapping key, ERP account code, account name, debit, credit and memo. Documents are rejected unless total debit equals total credit and each line has only one side populated.

## Default Mapping Keys

| Mapping key | Default code | Purpose |
| --- | --- | --- |
| `cash` | `1000` | Cash and payment clearing |
| `accounts_receivable` | `1100` | Customer receivables |
| `inventory` | `1200` | Inventory asset |
| `accounts_payable` | `2000` | Supplier payables |
| `tax_payable` | `2100` | Sales tax payable |
| `sales_revenue` | `4000` | Sales revenue |
| `sales_discount` | `4050` | Sales discounts |
| `cogs` | `5000` | Cost of goods sold |
| `expense` | `6000` | Operating expenses |

Specific overrides may use keys such as `cash:Card`, `cash:Mobile Payment`, or `expense:Rent`. When a specific mapping is absent, the prefix mapping is used.

Account mappings can be read and updated through the authorized `erp_accounting::list_mappings` and `save_mapping` core APIs. Mapping changes are audited. Replace the defaults with the ERP chart of accounts before enabling live accounting delivery.

## Reversals and Legacy Records

Refunds reverse the stored original sale lines rather than recalculating current settings. Expenses edited after posting receive a reversal followed by a new document. For a pre-Phase-4 sale refunded after rollout, the POS establishes its original balanced journal and reverses it in the same transaction.

## Installation

Phase 1 migration `010` must already be installed. Run the preflight, review migration `013_erp_accounting.sql`, then apply it with backup:

```powershell
./scripts/erp-phase4-database.ps1
./scripts/erp-phase4-database.ps1 -Apply
```

Do not deploy the Phase 4 application build before migration `013` is applied. Sales, refunds, purchases, customer collections and expenses intentionally require the accounting tables so the POS operation and its journal cannot diverge.
