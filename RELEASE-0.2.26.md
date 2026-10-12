# KAY POS 0.2.26

- Choose SQLite standalone or PostgreSQL server from Database connection.
- SQLite is bundled, with empty-shop setup, initial admin creation and online backups.
- Compact database dialog with mode buttons, inline file picker and equal admin fields.
- Database-scoped pending recovery and guards against switching with unfinished work.
- No automatic sync or shop-data import. Existing PostgreSQL configurations retain server mode.
- Verified PostgreSQL regression workflows plus standalone transactions, backups, credit, inventory, purchasing and payroll.
