# KAY POS 0.2.27

- Manual shop-data export from PostgreSQL into a complete standalone SQLite .db file.
- Import exports or standalone backups to a new local database from Settings > Database.
- Preserves accounts, linked records, images/attachments and invoice/batch numbering.
- Snapshot, row-count, integrity and foreign-key checks; existing files are never overwritten or merged.
- PostgreSQL data and the current database selection stay unchanged. No automatic sync.
