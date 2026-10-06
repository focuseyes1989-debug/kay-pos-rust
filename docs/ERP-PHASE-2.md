# ERP Phase 2: Master Data Pull

Phase 2 pulls canonical master data from one ERP endpoint. It creates stable ERP-to-POS identity mappings and stores a cursor after each atomic page import.

## Ownership Rules

ERP-owned fields:

- Category name, description and active status
- Product and variant names/options, SKU, barcode, prices, costs, low-stock thresholds, category, sale mode and description
- Customer identity, contact information, remarks and credit limit
- Supplier identity, contact, payment and active-status fields
- Location name

POS-owned fields are never overwritten by this import:

- Product and variant stock quantities
- Customer balance, points, visit count and total spent
- Sales, purchases and stock-movement history

Records are matched only through `rust_erp_entity_map`. An unmapped ERP record creates a new POS record; names, phone numbers and email addresses are not used for automatic merging.

## HTTP Contract

The POS sends `GET` with `limit=250` and, after the first page, `cursor=<value>`. Authentication uses the same optional bearer token as Phase 1.

```json
{
  "cursor": "next-page-token",
  "has_more": false,
  "categories": [
    {"external_id": "cat-1", "version": "4", "name": "Drinks", "description": "", "active": true}
  ],
  "products": [
    {
      "external_id": "product-1",
      "version": "12",
      "name": "Water",
      "category_external_id": "cat-1",
      "sku": "WATER-001",
      "barcode": "885000000001",
      "price": 1000,
      "cost": 700,
      "low_stock": 10,
      "sold_by": "Each",
      "description": "",
      "variants": []
    }
  ],
  "customers": [
    {"external_id": "customer-1", "name": "Customer", "phone": "", "email": "", "address": "", "remarks": "", "credit_limit": 0}
  ],
  "suppliers": [
    {"external_id": "supplier-1", "name": "Supplier", "active": true}
  ],
  "locations": [
    {"external_id": "location-1", "name": "Main Shop"}
  ]
}
```

Categories in a page are imported before products. A product category may also refer to a category mapped by an earlier page. Unknown category references reject and roll back the whole page.

Supported `sold_by` values are `Each`, `Weight`, `Service`, and `Variants`. The ERP cannot change this mode while the local product has stock.
Variant records are accepted only when the parent uses `sold_by=Variants`; their local stock quantities are preserved during metadata updates.

## Installation

Run the read-only preflight, review migration `011_erp_master_data.sql`, then apply with backup:

```powershell
./scripts/erp-phase2-database.ps1
./scripts/erp-phase2-database.ps1 -Apply
```

Enable the worker only when the endpoint is ready:

```dotenv
ZAY_POS_ERP_PROVIDER=default
ZAY_POS_ERP_MASTER_DATA_URL=https://erp.example.com/api/pos/master-data
ZAY_POS_ERP_MASTER_SYNC_INTERVAL_SECONDS=300
```

Leaving `ZAY_POS_ERP_MASTER_DATA_URL` empty disables inbound master sync.
