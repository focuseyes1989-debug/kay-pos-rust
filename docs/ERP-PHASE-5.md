# ERP Phase 5: Operations and Reliability

Phase 5 adds an administrator-only ERP Operations workspace to the Rust desktop client.

## Dashboard

The page shows pending, processing, delivered and dead outbox totals; the oldest queued event; last successful delivery; mapped master records; latest stock reconciliation; differences and unmapped stock rows.

Enabled workers report health after every cycle:

- `outbox-delivery`
- `master-data`
- `stock-reconciliation`

Worker errors are truncated before storage and API tokens or event payloads are never shown on the dashboard.

## Recovery Controls

Administrators can retry pending or dead events individually. Retry resets attempts and scheduling state, but preserves the original immutable payload and idempotency key. Every retry is written to `user_activity_log`.

The page also provides an audited editor for Chart of Accounts mappings. It does not expose ERP credentials.

## Access and Deployment

Only active `admin` sessions can open the workspace or execute retry and mapping changes. Server-side authorization is checked again inside each operation.

Migrations `010` through `013` must already be present. Run the preflight, review migration `014_erp_operations.sql`, then apply with backup:

```powershell
./scripts/erp-phase5-database.ps1
./scripts/erp-phase5-database.ps1 -Apply
```

The dashboard becomes available from `Settings > ERP Operations` after deploying the updated desktop build. A worker appears only when its corresponding endpoint is configured and it has completed at least one cycle.
