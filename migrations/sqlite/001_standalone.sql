-- Standalone schema version 1. Structure only; no shop records or credentials.

-- Mirrors the POS tables used by the existing PostgreSQL client.

CREATE TABLE IF NOT EXISTS "ai_dashboard_digests" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "digest_kind" TEXT NOT NULL,
    "period_start" TEXT NOT NULL,
    "period_end" TEXT NOT NULL,
    "generated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "created_by" INTEGER,
    "message" TEXT NOT NULL,
    "payload" TEXT NOT NULL,
    UNIQUE (digest_kind, period_start, period_end)
);

CREATE TABLE IF NOT EXISTS "ai_sales_summary_audit" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "user_id" INTEGER,
    "query_hash" TEXT NOT NULL,
    "result_type" TEXT NOT NULL,
    "data_scope" TEXT NOT NULL,
    "period_start" TEXT,
    "period_end" TEXT,
    "sources" TEXT,
    "response_ms" INTEGER NOT NULL DEFAULT 0,
    "success" INTEGER NOT NULL DEFAULT 1,
    "generated_at" TEXT DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "ai_sales_summary_digests" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "digest_kind" TEXT NOT NULL,
    "period_start" TEXT NOT NULL,
    "period_end" TEXT NOT NULL,
    "data_scope" TEXT NOT NULL,
    "owner_key" TEXT NOT NULL,
    "created_by" INTEGER,
    "message" TEXT NOT NULL,
    "payload" TEXT NOT NULL,
    "generated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (digest_kind, period_start, period_end, data_scope, owner_key)
);

CREATE TABLE IF NOT EXISTS "app_metadata" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "app_version" TEXT,
    "db_version" TEXT,
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "notes" TEXT
);

CREATE TABLE IF NOT EXISTS "attendance" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER NOT NULL,
    "attendance_date" TEXT NOT NULL,
    "check_in" TEXT,
    "check_out" TEXT,
    "status" TEXT DEFAULT 'Present',
    "late_minutes" INTEGER DEFAULT 0,
    "notes" TEXT,
    "corrected_by" INTEGER,
    "correction_reason" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (employee_id, attendance_date),
    FOREIGN KEY (corrected_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "car_print_audit" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "job_public_id" TEXT,
    "event" TEXT NOT NULL,
    "detail" TEXT,
    "actor" TEXT NOT NULL,
    "created_at" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "car_print_jobs" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "public_id" TEXT NOT NULL,
    "request_key" TEXT NOT NULL,
    "car_id" INTEGER NOT NULL,
    "page_sequence" TEXT NOT NULL,
    "copies" INTEGER NOT NULL DEFAULT 1,
    "status" TEXT NOT NULL DEFAULT 'pending',
    "error_message" TEXT,
    "requested_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    "printer_name" TEXT,
    UNIQUE (public_id),
    UNIQUE (request_key),
    FOREIGN KEY (car_id) REFERENCES cars(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "car_print_printers" (
    "printer_name" TEXT NOT NULL,
    "client_name" TEXT NOT NULL,
    "is_default" INTEGER NOT NULL DEFAULT 0,
    "last_seen" TEXT NOT NULL,
    PRIMARY KEY (printer_name)
);

CREATE TABLE IF NOT EXISTS "car_qr_tokens" (
    "car_id" INTEGER NOT NULL,
    "token" TEXT NOT NULL,
    "is_active" INTEGER NOT NULL DEFAULT 1,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY (car_id),
    UNIQUE (token),
    FOREIGN KEY (car_id) REFERENCES cars(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "cars" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "car_number" TEXT NOT NULL,
    "driver_name" TEXT NOT NULL,
    "kind_of_car" TEXT,
    "type_of_car" TEXT,
    "age" TEXT,
    "nrc_place" TEXT,
    "nrc_number" TEXT NOT NULL,
    "phone_number" TEXT,
    "address" TEXT,
    "engine_number" TEXT,
    "frame_number" TEXT,
    "timestamp" TEXT
);

CREATE TABLE IF NOT EXISTS "cars_age_cleanup_backup_20260819_1252" (
    "id" INTEGER,
    "age" TEXT,
    "backed_up_at" TEXT
);

CREATE TABLE IF NOT EXISTS "cash_drawer" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "is_active" INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS "cash_sessions" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER,
    "opened_at" TEXT NOT NULL,
    "opening_cash" REAL DEFAULT 0,
    "closed_at" TEXT,
    "expected_cash" REAL,
    "actual_cash" REAL,
    "difference" REAL,
    "status" TEXT DEFAULT 'Open',
    "notes" TEXT,
    "opened_by" INTEGER,
    "closed_by" INTEGER,
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS "categories" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "slug" TEXT,
    "description" TEXT,
    "parent_id" INTEGER,
    "sort_order" INTEGER DEFAULT 0,
    "color" TEXT DEFAULT '#6c5ce7',
    "icon" TEXT DEFAULT '',
    "image_path" TEXT,
    "status" TEXT DEFAULT 'active',
    "code" TEXT,
    "notes" TEXT,
    "is_system" INTEGER DEFAULT 0,
    "is_favorite" INTEGER DEFAULT 0,
    "group_id" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "image" TEXT,
    FOREIGN KEY (parent_id) REFERENCES categories(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "category_activity_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "category_id" INTEGER,
    "action" TEXT,
    "details" TEXT,
    "user_id" INTEGER,
    "username" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "category_groups" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "sort_order" INTEGER DEFAULT 0,
    "icon" TEXT DEFAULT '',
    "color" TEXT DEFAULT '#6c5ce7',
    "is_favorite" INTEGER DEFAULT 0,
    "is_active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "category_stats" (
    "category_id" INTEGER NOT NULL,
    "product_count" INTEGER DEFAULT 0,
    "last_updated" TEXT DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (category_id)
);

CREATE TABLE IF NOT EXISTS "commission_rules" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER,
    "rate_percent" REAL DEFAULT 0,
    "target_amount" REAL DEFAULT 0,
    "active" INTEGER DEFAULT 1,
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (employee_id),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "credit_adjustments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "customer_id" INTEGER,
    "credit_sale_id" INTEGER,
    "amount" REAL DEFAULT 0,
    "adjustment_type" TEXT,
    "reason" TEXT,
    "reference_no" TEXT,
    "created_by" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "credit_payments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "credit_sale_id" INTEGER NOT NULL,
    "customer_id" INTEGER NOT NULL,
    "amount" REAL NOT NULL DEFAULT 0,
    "payment_date" TEXT NOT NULL,
    "payment_method" TEXT DEFAULT 'Cash',
    "reference_no" TEXT,
    "note" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (credit_sale_id) REFERENCES credit_sales(id) ON DELETE CASCADE,
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS "credit_sales" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "invoice_no" TEXT NOT NULL,
    "customer_id" INTEGER NOT NULL,
    "total_amount" REAL NOT NULL DEFAULT 0,
    "paid_amount" REAL DEFAULT 0,
    "balance_amount" REAL NOT NULL DEFAULT 0,
    "sale_date" TEXT NOT NULL,
    "due_date" TEXT,
    "status" TEXT DEFAULT 'pending',
    "notes" TEXT,
    "sale_id" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT,
    "refunded_at" TEXT,
    "refund_reason" TEXT,
    "refund_type" TEXT,
    UNIQUE (invoice_no),
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE RESTRICT,
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "credit_transactions" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "credit_sale_id" INTEGER,
    "customer_id" INTEGER,
    "amount" REAL DEFAULT 0,
    "transaction_type" TEXT,
    "reference_no" TEXT,
    "notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "type" TEXT,
    "description" TEXT,
    "payment_id" INTEGER,
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "customer_points_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "customer_id" INTEGER NOT NULL,
    "points" INTEGER NOT NULL,
    "type" TEXT NOT NULL,
    "reference" TEXT,
    "expiry_date" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "customers" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "phone" TEXT,
    "email" TEXT,
    "address" TEXT,
    "total_visit" INTEGER DEFAULT 0,
    "total_spent" REAL DEFAULT 0,
    "points" INTEGER DEFAULT 0,
    "credit_limit" REAL DEFAULT 0,
    "current_balance" REAL DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "remarks" TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS "employee_documents" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER,
    "document_type" TEXT NOT NULL,
    "document_no" TEXT,
    "file_path" TEXT,
    "issued_date" TEXT,
    "expiry_date" TEXT,
    "notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "employee_leave" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER,
    "leave_type" TEXT NOT NULL,
    "start_date" TEXT NOT NULL,
    "end_date" TEXT NOT NULL,
    "days" REAL DEFAULT 1,
    "reason" TEXT,
    "status" TEXT DEFAULT 'Pending',
    "reviewed_by" INTEGER,
    "reviewed_at" TEXT,
    "review_notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE,
    FOREIGN KEY (reviewed_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "employee_shifts" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER NOT NULL,
    "shift_id" INTEGER NOT NULL,
    "effective_from" TEXT NOT NULL,
    "effective_to" TEXT,
    "weekly_off_days" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE,
    FOREIGN KEY (shift_id) REFERENCES shifts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "employees" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_no" TEXT NOT NULL,
    "user_id" INTEGER,
    "full_name" TEXT NOT NULL,
    "phone" TEXT,
    "address" TEXT,
    "date_of_birth" TEXT,
    "national_id" TEXT,
    "photo_path" TEXT,
    "hire_date" TEXT NOT NULL,
    "position" TEXT,
    "department" TEXT,
    "branch" TEXT,
    "employment_status" TEXT DEFAULT 'Active',
    "emergency_contact_name" TEXT,
    "emergency_contact_phone" TEXT,
    "notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "zkteco_user_id" TEXT,
    "photo_data" BLOB,
    UNIQUE (employee_no),
    UNIQUE (user_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "expense_alerts_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "category" TEXT,
    "month" INTEGER,
    "year" INTEGER,
    "budget_amount" REAL DEFAULT 0,
    "actual_amount" REAL DEFAULT 0,
    "used_percentage" REAL DEFAULT 0,
    "alert_type" TEXT,
    "message" TEXT,
    "is_read" INTEGER DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "expense_attachments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "expense_id" INTEGER,
    "filename" TEXT,
    "file_path" TEXT,
    "file_size" INTEGER DEFAULT 0,
    "mime_type" TEXT,
    "uploaded_by" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (expense_id) REFERENCES expenses(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "expense_budgets" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "category" TEXT NOT NULL,
    "month" INTEGER NOT NULL,
    "year" INTEGER NOT NULL,
    "budget_amount" REAL DEFAULT 0,
    "notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "expense_categories" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "is_active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "expense_notification_settings" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "enable_notifications" INTEGER DEFAULT 1,
    "warning_threshold" INTEGER DEFAULT 80,
    "check_frequency" TEXT DEFAULT 'daily',
    "last_checked" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "expenses" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "expense_no" TEXT,
    "category" TEXT NOT NULL,
    "description" TEXT,
    "amount" REAL DEFAULT 0,
    "expense_date" TEXT,
    "payment_method" TEXT,
    "reference_no" TEXT,
    "notes" TEXT,
    "created_by" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "image" TEXT,
    UNIQUE (expense_no)
);

CREATE TABLE IF NOT EXISTS "expiry_alerts_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER,
    "product_name" TEXT,
    "location" TEXT,
    "batch_no" TEXT,
    "expire_date" TEXT,
    "quantity" INTEGER DEFAULT 0,
    "alert_type" TEXT,
    "message" TEXT,
    "sale_id" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "is_resolved" INTEGER DEFAULT 0,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE SET NULL,
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "held_sales" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "hold_no" TEXT,
    "cart_json" TEXT NOT NULL,
    "customer_id" INTEGER,
    "customer_name" TEXT,
    "payment_type" TEXT,
    "note" TEXT,
    "total_amount" REAL DEFAULT 0,
    "item_count" INTEGER DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (hold_no),
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "locations" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "migration_history" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "version" TEXT,
    "name" TEXT,
    "description" TEXT,
    "status" TEXT,
    "applied_at" TEXT,
    "rolled_back_at" TEXT,
    "executed_by" TEXT,
    "execution_time" REAL DEFAULT 0,
    "error_message" TEXT
);

CREATE TABLE IF NOT EXISTS "migrations" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "version" TEXT,
    "name" TEXT,
    "applied_at" TEXT DEFAULT (kay_clock_timestamp()),
    "description" TEXT
);

CREATE TABLE IF NOT EXISTS "native_admin_requests" (
    "request_id" TEXT NOT NULL,
    "user_id" INTEGER NOT NULL,
    "operation" TEXT NOT NULL,
    "payload_hash" TEXT NOT NULL,
    "result_json" TEXT NOT NULL DEFAULT '',
    "created_at" TEXT NOT NULL,
    PRIMARY KEY (request_id)
);

CREATE TABLE IF NOT EXISTS "native_catalog_requests" (
    "request_id" TEXT NOT NULL,
    "user_id" INTEGER NOT NULL,
    "operation" TEXT NOT NULL,
    "payload_hash" TEXT NOT NULL,
    "result_json" TEXT NOT NULL DEFAULT '',
    "created_at" TEXT NOT NULL,
    PRIMARY KEY (request_id)
);

CREATE TABLE IF NOT EXISTS "network_print_audit" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "job_id" TEXT NOT NULL,
    "event" TEXT NOT NULL,
    "detail" TEXT,
    "actor" TEXT NOT NULL,
    "created_at" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "network_print_jobs" (
    "job_id" TEXT NOT NULL,
    "request_key" TEXT NOT NULL,
    "source_agent_id" TEXT,
    "target_agent_id" TEXT NOT NULL,
    "printer_name" TEXT NOT NULL,
    "job_type" TEXT NOT NULL,
    "payload_json" TEXT NOT NULL,
    "copies" INTEGER NOT NULL DEFAULT 1,
    "status" TEXT NOT NULL DEFAULT 'pending',
    "attempts" INTEGER NOT NULL DEFAULT 0,
    "max_attempts" INTEGER NOT NULL DEFAULT 3,
    "error_message" TEXT,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    "claimed_at" TEXT,
    "completed_at" TEXT,
    PRIMARY KEY (job_id),
    UNIQUE (request_key)
);

CREATE TABLE IF NOT EXISTS "network_printers" (
    "agent_id" TEXT NOT NULL,
    "printer_name" TEXT NOT NULL,
    "is_default" INTEGER NOT NULL DEFAULT 0,
    "status" TEXT NOT NULL DEFAULT 'online',
    "last_seen" TEXT NOT NULL,
    "is_enabled" INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (agent_id, printer_name),
    FOREIGN KEY (agent_id) REFERENCES printer_agents(agent_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "payment_types" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "active" INTEGER DEFAULT 1,
    "sort_order" INTEGER DEFAULT 0,
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "payments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "sale_id" INTEGER,
    "customer_id" INTEGER,
    "payment_type" TEXT,
    "amount" REAL DEFAULT 0,
    "cash_drawer_id" INTEGER,
    "payment_date" TEXT DEFAULT (kay_clock_timestamp()),
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE SET NULL,
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "payrolls" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "payroll_no" TEXT NOT NULL,
    "employee_id" INTEGER NOT NULL,
    "period_month" TEXT NOT NULL,
    "basic_salary" REAL DEFAULT 0,
    "allowance" REAL DEFAULT 0,
    "overtime_amount" REAL DEFAULT 0,
    "bonus" REAL DEFAULT 0,
    "late_deduction" REAL DEFAULT 0,
    "absence_deduction" REAL DEFAULT 0,
    "advance_deduction" REAL DEFAULT 0,
    "other_deduction" REAL DEFAULT 0,
    "net_salary" REAL DEFAULT 0,
    "status" TEXT DEFAULT 'Draft',
    "paid_date" TEXT,
    "payment_method" TEXT,
    "expense_id" INTEGER,
    "notes" TEXT,
    "created_by" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (employee_id, period_month),
    UNIQUE (payroll_no),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE RESTRICT,
    FOREIGN KEY (expense_id) REFERENCES expenses(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "print_service_presets" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "product_id" INTEGER NOT NULL,
    "description" TEXT,
    "pricing_unit" TEXT NOT NULL DEFAULT 'per_item',
    "unit_price" REAL NOT NULL DEFAULT 0,
    "pages_per_copy" INTEGER NOT NULL DEFAULT 1,
    "copy_count" INTEGER NOT NULL DEFAULT 1,
    "paper_size" TEXT,
    "paper_type" TEXT,
    "color_mode" TEXT NOT NULL DEFAULT 'not_applicable',
    "print_side" TEXT NOT NULL DEFAULT 'not_applicable',
    "finishing" TEXT,
    "sort_order" INTEGER NOT NULL DEFAULT 0,
    "active" INTEGER NOT NULL DEFAULT 1,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "printer_agents" (
    "agent_id" TEXT NOT NULL,
    "computer_name" TEXT NOT NULL,
    "ip_address" TEXT NOT NULL,
    "platform" TEXT,
    "agent_version" TEXT,
    "last_seen" TEXT NOT NULL,
    "api_key_hash" TEXT,
    "is_enabled" INTEGER NOT NULL DEFAULT 1,
    "allowed_job_types" TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (agent_id)
);

CREATE TABLE IF NOT EXISTS "printer_security_audit" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "event" TEXT NOT NULL,
    "agent_id" TEXT,
    "detail" TEXT,
    "created_at" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "product_discounts" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER NOT NULL,
    "discount_percent" REAL NOT NULL DEFAULT 0,
    "start_date" TEXT NOT NULL,
    "end_date" TEXT NOT NULL,
    "active" INTEGER NOT NULL DEFAULT 1,
    "note" TEXT,
    "discount_type" TEXT DEFAULT 'percentage',
    "manual_price" REAL DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "product_locations" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER NOT NULL,
    "quantity" REAL DEFAULT 0,
    "location" TEXT DEFAULT '',
    "batch_no" TEXT DEFAULT '',
    "expire_date" TEXT,
    "manufacture_date" TEXT,
    "expiry_discount_enabled" INTEGER DEFAULT 0,
    "expiry_discount_percent" REAL DEFAULT 0,
    "expiry_discount_start_date" TEXT,
    "expiry_discount_end_date" TEXT,
    "clearance_note" TEXT,
    "last_updated" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (product_id, location, batch_no, expire_date),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "product_price_tiers" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER NOT NULL,
    "min_qty" INTEGER NOT NULL DEFAULT 1,
    "unit_label" TEXT,
    "unit_multiplier" INTEGER DEFAULT 1,
    "barcode" TEXT,
    "unit_price" REAL NOT NULL DEFAULT 0,
    "note" TEXT,
    "active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "product_variants" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER NOT NULL,
    "size" TEXT,
    "color" TEXT,
    "sku" TEXT,
    "barcode" TEXT,
    "price" REAL DEFAULT 0,
    "cost" REAL DEFAULT 0,
    "stock" INTEGER DEFAULT 0,
    "low_stock" INTEGER DEFAULT 0,
    "image" TEXT,
    "active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "wholesale_min_qty" INTEGER DEFAULT 0,
    "wholesale_price" REAL DEFAULT 0,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "products" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "category" TEXT,
    "description" TEXT,
    "sold_by" TEXT DEFAULT 'Each',
    "price" REAL DEFAULT 0,
    "cost" REAL DEFAULT 0,
    "sku" TEXT,
    "barcode" TEXT,
    "stock" REAL DEFAULT 0,
    "expire_date" TEXT,
    "low_stock" REAL DEFAULT 0,
    "image" TEXT,
    "image_data" BLOB,
    "image_mime" TEXT,
    "image_filename" TEXT,
    "supplier_id" INTEGER,
    "unit" TEXT,
    "base_unit" TEXT DEFAULT 'pcs',
    "pack_unit" TEXT DEFAULT '',
    "pack_size" INTEGER DEFAULT 1,
    "restaurant_modifiers" TEXT,
    "warehouse" TEXT,
    "batch_no" TEXT,
    "manufacture_date" TEXT,
    "last_updated" TEXT DEFAULT (kay_clock_timestamp()),
    "is_favourite" INTEGER DEFAULT 0,
    "category_id" INTEGER
);

CREATE TABLE IF NOT EXISTS "purchase_order_items" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "po_id" INTEGER,
    "product_id" INTEGER,
    "quantity" REAL DEFAULT 0,
    "unit_price" REAL DEFAULT 0,
    "total" REAL DEFAULT 0,
    FOREIGN KEY (po_id) REFERENCES purchase_orders(id) ON DELETE CASCADE,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "purchase_orders" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "po_no" TEXT,
    "supplier_id" INTEGER,
    "order_date" TEXT,
    "total_amount" REAL DEFAULT 0,
    "status" TEXT DEFAULT 'pending',
    "discount" REAL DEFAULT 0,
    "tax" REAL DEFAULT 0,
    "payment_status" TEXT DEFAULT 'Unpaid',
    "received_by" TEXT,
    "invoice_attachment" TEXT,
    "notes" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (po_no),
    FOREIGN KEY (supplier_id) REFERENCES suppliers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "restaurant_kitchen_ticket_items" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "ticket_id" INTEGER NOT NULL,
    "order_item_id" INTEGER,
    "product_id" INTEGER,
    "product_name" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "modifier_summary" TEXT,
    "quantity" REAL DEFAULT 0,
    "note" TEXT,
    "status" TEXT DEFAULT 'sent',
    "sort_order" INTEGER DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    "preparing_at" TEXT,
    "ready_at" TEXT,
    "served_at" TEXT,
    "cancelled_at" TEXT,
    FOREIGN KEY (order_item_id) REFERENCES restaurant_order_items(id) ON DELETE SET NULL,
    FOREIGN KEY (ticket_id) REFERENCES restaurant_kitchen_tickets(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "restaurant_kitchen_tickets" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "ticket_no" TEXT NOT NULL,
    "order_id" INTEGER NOT NULL,
    "status" TEXT DEFAULT 'sent',
    "ticket_signature" TEXT NOT NULL,
    "printed" INTEGER DEFAULT 0,
    "note" TEXT,
    "completed_at" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (ticket_no),
    FOREIGN KEY (order_id) REFERENCES restaurant_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "restaurant_order_items" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "order_id" INTEGER NOT NULL,
    "product_id" INTEGER,
    "product_name" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "quantity" REAL DEFAULT 0,
    "unit_price" REAL DEFAULT 0,
    "base_price" REAL DEFAULT 0,
    "line_total" REAL DEFAULT 0,
    "note" TEXT,
    "line_id" TEXT,
    "status" TEXT DEFAULT 'active',
    "kitchen_status" TEXT DEFAULT 'draft',
    "sent_quantity" REAL DEFAULT 0,
    "cancelled_quantity" REAL DEFAULT 0,
    "sort_order" INTEGER DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (order_id) REFERENCES restaurant_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "restaurant_order_modifiers" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "order_item_id" INTEGER NOT NULL,
    "group_name" TEXT,
    "modifier_name" TEXT NOT NULL,
    "modifier_type" TEXT DEFAULT 'note',
    "price_delta" REAL DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (order_item_id) REFERENCES restaurant_order_items(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "restaurant_orders" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "order_no" TEXT NOT NULL,
    "table_id" INTEGER,
    "order_type" TEXT DEFAULT 'Dine-in',
    "status" TEXT DEFAULT 'open',
    "kitchen_status" TEXT DEFAULT 'draft',
    "cart_json" TEXT NOT NULL,
    "customer_id" INTEGER,
    "customer_name" TEXT,
    "note" TEXT,
    "total_amount" REAL DEFAULT 0,
    "item_count" INTEGER DEFAULT 0,
    "opened_at" TEXT DEFAULT (kay_clock_timestamp()),
    "sent_to_kitchen_at" TEXT,
    "settled_at" TEXT,
    "cancelled_at" TEXT,
    "sale_id" INTEGER,
    "invoice_no" TEXT,
    "settled_total" REAL DEFAULT 0,
    "payment_amount" REAL DEFAULT 0,
    "change_amount" REAL DEFAULT 0,
    "payment_type" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (order_no),
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE SET NULL,
    FOREIGN KEY (table_id) REFERENCES restaurant_tables(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "restaurant_tables" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "table_no" TEXT NOT NULL,
    "display_name" TEXT,
    "seats" INTEGER DEFAULT 4,
    "status" TEXT DEFAULT 'available',
    "sort_order" INTEGER DEFAULT 0,
    "active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (table_no)
);

CREATE TABLE IF NOT EXISTS "rust_checkout_requests" (
    "request_id" TEXT NOT NULL,
    "fingerprint" TEXT NOT NULL,
    "sale_id" INTEGER,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (request_id),
    FOREIGN KEY (sale_id) REFERENCES sales(id)
);

CREATE TABLE IF NOT EXISTS "rust_employee_requests" (
    "request_id" TEXT NOT NULL,
    "user_id" INTEGER NOT NULL,
    "operation" TEXT NOT NULL,
    "payload_hash" TEXT NOT NULL,
    "record_id" INTEGER NOT NULL,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (request_id),
    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS "rust_erp_account_mappings" (
    "mapping_key" TEXT NOT NULL,
    "account_code" TEXT NOT NULL,
    "account_name" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (mapping_key)
);

CREATE TABLE IF NOT EXISTS "rust_erp_accounting_documents" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "event_key" TEXT NOT NULL,
    "event_type" TEXT NOT NULL,
    "source_type" TEXT NOT NULL,
    "source_id" INTEGER NOT NULL,
    "document_date" TEXT NOT NULL,
    "currency" TEXT NOT NULL DEFAULT 'MMK',
    "total_debit" NUMERIC NOT NULL,
    "total_credit" NUMERIC NOT NULL,
    "lines" TEXT NOT NULL,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    UNIQUE (event_key)
);

CREATE TABLE IF NOT EXISTS "rust_erp_entity_map" (
    "provider" TEXT NOT NULL,
    "entity_type" TEXT NOT NULL,
    "external_id" TEXT NOT NULL,
    "local_id" INTEGER NOT NULL,
    "source_version" TEXT,
    "source_hash" TEXT NOT NULL,
    "synced_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (provider, entity_type, external_id),
    UNIQUE (provider, entity_type, local_id)
);

CREATE TABLE IF NOT EXISTS "rust_erp_import_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "provider" TEXT NOT NULL,
    "cursor_value" TEXT,
    "categories_count" INTEGER NOT NULL DEFAULT 0,
    "products_count" INTEGER NOT NULL DEFAULT 0,
    "customers_count" INTEGER NOT NULL DEFAULT 0,
    "suppliers_count" INTEGER NOT NULL DEFAULT 0,
    "locations_count" INTEGER NOT NULL DEFAULT 0,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "rust_erp_outbox" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "event_key" TEXT NOT NULL,
    "event_type" TEXT NOT NULL,
    "aggregate_type" TEXT NOT NULL,
    "aggregate_id" TEXT NOT NULL,
    "payload" TEXT NOT NULL,
    "status" TEXT NOT NULL DEFAULT 'pending',
    "attempts" INTEGER NOT NULL DEFAULT 0,
    "next_attempt_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "locked_until" TEXT,
    "last_error" TEXT,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "sent_at" TEXT,
    UNIQUE (event_key)
);

CREATE TABLE IF NOT EXISTS "rust_erp_reconciliation_runs" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "provider" TEXT NOT NULL,
    "snapshot_id" TEXT NOT NULL,
    "status" TEXT NOT NULL DEFAULT 'running',
    "compared_count" INTEGER NOT NULL DEFAULT 0,
    "difference_count" INTEGER NOT NULL DEFAULT 0,
    "unmapped_count" INTEGER NOT NULL DEFAULT 0,
    "cursor_value" TEXT,
    "last_error" TEXT,
    "started_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "completed_at" TEXT,
    UNIQUE (provider, snapshot_id)
);

CREATE TABLE IF NOT EXISTS "rust_erp_runtime" (
    "id" INTEGER NOT NULL,
    "enabled" INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS "rust_erp_stock_discrepancies" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "run_id" INTEGER NOT NULL,
    "item_key" TEXT NOT NULL,
    "product_external_id" TEXT NOT NULL,
    "variant_external_id" TEXT,
    "location_external_id" TEXT,
    "local_product_id" INTEGER,
    "local_variant_id" INTEGER,
    "local_location_id" INTEGER,
    "erp_quantity" REAL NOT NULL,
    "pos_quantity" REAL,
    "difference" REAL,
    "status" TEXT NOT NULL,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    UNIQUE (run_id, item_key),
    FOREIGN KEY (run_id) REFERENCES rust_erp_reconciliation_runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "rust_erp_sync_state" (
    "provider" TEXT NOT NULL,
    "stream" TEXT NOT NULL,
    "cursor_value" TEXT,
    "last_started_at" TEXT,
    "last_succeeded_at" TEXT,
    "last_error" TEXT,
    PRIMARY KEY (provider, stream)
);

CREATE TABLE IF NOT EXISTS "rust_erp_worker_health" (
    "worker_name" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "last_started_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "last_finished_at" TEXT,
    "last_success_at" TEXT,
    "last_error" TEXT,
    "updated_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (worker_name)
);

CREATE TABLE IF NOT EXISTS "rust_held_requests" (
    "request_id" TEXT NOT NULL,
    "username" TEXT NOT NULL,
    "payload_hash" TEXT NOT NULL,
    "result_json" TEXT NOT NULL,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (request_id)
);

CREATE TABLE IF NOT EXISTS "rust_held_sessions" (
    "token" TEXT NOT NULL,
    "username" TEXT NOT NULL,
    "held_id" INTEGER NOT NULL,
    "body" TEXT NOT NULL,
    "state" TEXT NOT NULL DEFAULT 'active',
    "sale_id" INTEGER,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (token),
    UNIQUE (sale_id),
    FOREIGN KEY (sale_id) REFERENCES sales(id)
);

CREATE TABLE IF NOT EXISTS "rust_purchase_drafts" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "po_no" TEXT NOT NULL,
    "supplier_id" INTEGER NOT NULL,
    "order_date" TEXT NOT NULL,
    "body" TEXT NOT NULL,
    "total_amount" NUMERIC NOT NULL,
    "status" TEXT NOT NULL DEFAULT 'pending',
    "revision" INTEGER NOT NULL DEFAULT 1,
    "po_id" INTEGER,
    "created_by" TEXT NOT NULL,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    "updated_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    UNIQUE (po_no),
    UNIQUE (po_id),
    FOREIGN KEY (supplier_id) REFERENCES suppliers(id),
    FOREIGN KEY (po_id) REFERENCES purchase_orders(id)
);

CREATE TABLE IF NOT EXISTS "rust_purchase_items" (
    "po_item_id" INTEGER NOT NULL,
    "variant_id" INTEGER,
    "location" TEXT NOT NULL,
    "batch_no" TEXT NOT NULL,
    "expire_date" TEXT NOT NULL,
    PRIMARY KEY (po_item_id),
    FOREIGN KEY (po_item_id) REFERENCES purchase_order_items(id),
    FOREIGN KEY (variant_id) REFERENCES product_variants(id)
);

CREATE TABLE IF NOT EXISTS "rust_purchase_movements" (
    "movement_id" INTEGER NOT NULL,
    "po_id" INTEGER NOT NULL,
    PRIMARY KEY (movement_id),
    FOREIGN KEY (movement_id) REFERENCES stock_movements(id),
    FOREIGN KEY (po_id) REFERENCES purchase_orders(id)
);

CREATE TABLE IF NOT EXISTS "rust_purchase_requests" (
    "request_id" TEXT NOT NULL,
    "username" TEXT NOT NULL,
    "payload_hash" TEXT NOT NULL,
    "result_id" INTEGER,
    "cancelled" INTEGER NOT NULL DEFAULT 0,
    "created_at" TEXT NOT NULL DEFAULT (kay_clock_timestamp()),
    PRIMARY KEY (request_id)
);

CREATE TABLE IF NOT EXISTS "rust_sale_points" (
    "sale_id" INTEGER NOT NULL,
    "customer_id" INTEGER NOT NULL,
    "earned" INTEGER NOT NULL,
    "refunded" INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (sale_id),
    FOREIGN KEY (sale_id) REFERENCES sales(id),
    FOREIGN KEY (customer_id) REFERENCES customers(id)
);

CREATE TABLE IF NOT EXISTS "salary_advances" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "employee_id" INTEGER,
    "advance_date" TEXT NOT NULL,
    "amount" REAL NOT NULL,
    "repaid_amount" REAL DEFAULT 0,
    "status" TEXT DEFAULT 'Outstanding',
    "notes" TEXT,
    "created_by" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS "sale_items" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "sale_id" INTEGER,
    "product_id" INTEGER,
    "product_name" TEXT,
    "qty" REAL DEFAULT 0,
    "price" REAL DEFAULT 0,
    "total" REAL DEFAULT 0,
    "cost" REAL DEFAULT 0,
    "variant_id" INTEGER,
    "location_id" INTEGER,
    "location" TEXT,
    "batch_no" TEXT,
    "expire_date" TEXT,
    "wholesale_regular_price" REAL DEFAULT 0,
    "wholesale_savings" REAL DEFAULT 0,
    "wholesale_tier_min_qty" INTEGER,
    "wholesale_unit_label" TEXT,
    "refunded_qty" REAL DEFAULT 0,
    "refund_reason" TEXT,
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "sales" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "invoice_no" TEXT,
    "total" REAL DEFAULT 0,
    "payment" REAL DEFAULT 0,
    "change_amount" REAL DEFAULT 0,
    "customer_id" INTEGER,
    "status" TEXT DEFAULT 'completed',
    "payment_type" TEXT,
    "discount_amount" REAL DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "cogs" REAL DEFAULT 0,
    "gross_profit" REAL DEFAULT 0,
    "net_profit" REAL DEFAULT 0,
    "created_by" TEXT,
    UNIQUE (invoice_no)
);

CREATE TABLE IF NOT EXISTS "service_order_design_prompts" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "title" TEXT NOT NULL,
    "prompt_text" TEXT NOT NULL,
    "sort_order" INTEGER NOT NULL DEFAULT 0,
    "active" INTEGER NOT NULL DEFAULT 1,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    "image_data" TEXT,
    "image_name" TEXT,
    "category" TEXT NOT NULL DEFAULT '',
    UNIQUE (title)
);

CREATE TABLE IF NOT EXISTS "service_order_history" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "action" TEXT NOT NULL,
    "note" TEXT,
    "created_by" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_order_items" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "product_id" INTEGER,
    "variant_id" INTEGER,
    "item_type" TEXT NOT NULL DEFAULT 'service',
    "description" TEXT NOT NULL,
    "qty" REAL NOT NULL DEFAULT 1,
    "unit_price" REAL NOT NULL DEFAULT 0,
    "estimated_cost" REAL NOT NULL DEFAULT 0,
    "actual_cost" REAL NOT NULL DEFAULT 0,
    "warranty_days" INTEGER NOT NULL DEFAULT 0,
    "pricing_unit" TEXT NOT NULL DEFAULT 'per_item',
    "pages_per_copy" INTEGER NOT NULL DEFAULT 1,
    "copy_count" INTEGER NOT NULL DEFAULT 1,
    "total_sheets" INTEGER NOT NULL DEFAULT 1,
    "paper_size" TEXT,
    "paper_type" TEXT,
    "color_mode" TEXT NOT NULL DEFAULT 'not_applicable',
    "print_side" TEXT NOT NULL DEFAULT 'not_applicable',
    "finishing" TEXT,
    "file_name" TEXT,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_order_notifications" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "event" TEXT NOT NULL,
    "channel" TEXT NOT NULL DEFAULT 'queue',
    "recipient" TEXT,
    "message" TEXT NOT NULL,
    "status" TEXT NOT NULL DEFAULT 'pending',
    "attempts" INTEGER NOT NULL DEFAULT 0,
    "created_at" TEXT NOT NULL,
    "sent_at" TEXT,
    "error_message" TEXT,
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_order_payments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "payment_type" TEXT NOT NULL,
    "amount" REAL NOT NULL,
    "reference_no" TEXT,
    "note" TEXT,
    "received_by" TEXT NOT NULL,
    "received_at" TEXT NOT NULL,
    "sale_id" INTEGER,
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_order_return_visits" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "reason" TEXT NOT NULL,
    "resolution" TEXT,
    "status" TEXT NOT NULL DEFAULT 'open',
    "handled_by" TEXT,
    "visited_at" TEXT NOT NULL,
    "closed_at" TEXT,
    "created_by" TEXT NOT NULL,
    "created_at" TEXT NOT NULL,
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_order_status_history" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "service_order_id" INTEGER NOT NULL,
    "from_status" TEXT,
    "to_status" TEXT NOT NULL,
    "note" TEXT,
    "changed_by" TEXT NOT NULL,
    "changed_at" TEXT NOT NULL,
    FOREIGN KEY (service_order_id) REFERENCES service_orders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "service_orders" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "order_no" TEXT NOT NULL,
    "customer_id" INTEGER,
    "customer_name" TEXT,
    "customer_phone" TEXT,
    "status" TEXT NOT NULL DEFAULT 'received',
    "priority" TEXT NOT NULL DEFAULT 'normal',
    "assigned_to" TEXT,
    "received_at" TEXT NOT NULL,
    "expected_at" TEXT,
    "completed_at" TEXT,
    "delivered_at" TEXT,
    "item_name" TEXT,
    "item_model" TEXT,
    "job_title" TEXT,
    "file_source" TEXT,
    "file_reference" TEXT,
    "approval_status" TEXT NOT NULL DEFAULT 'not_required',
    "serial_no" TEXT,
    "accessories" TEXT,
    "condition_notes" TEXT,
    "complaint" TEXT,
    "diagnosis" TEXT,
    "internal_notes" TEXT,
    "deposit_amount" REAL NOT NULL DEFAULT 0,
    "sale_id" INTEGER,
    "checkout_started_at" TEXT,
    "sale_refunded_at" TEXT,
    "created_by" TEXT NOT NULL,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    "completed_by" TEXT,
    "started_by" TEXT,
    "started_at" TEXT,
    "delivered_by" TEXT,
    UNIQUE (order_no)
);

CREATE TABLE IF NOT EXISTS "settings" (
    "key" TEXT NOT NULL,
    "value" TEXT,
    PRIMARY KEY (key)
);

CREATE TABLE IF NOT EXISTS "shifts" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "start_time" TEXT NOT NULL,
    "end_time" TEXT NOT NULL,
    "break_minutes" INTEGER DEFAULT 0,
    "is_overnight" INTEGER DEFAULT 0,
    "is_active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "stock_movements" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "product_id" INTEGER,
    "type" TEXT,
    "quantity" REAL,
    "old_stock" REAL,
    "new_stock" REAL,
    "reason" TEXT,
    "reference" TEXT,
    "created_by" TEXT,
    "notes" TEXT,
    "supplier_id" INTEGER,
    "location" TEXT,
    "customer_id" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    "variant_id" INTEGER,
    FOREIGN KEY (customer_id) REFERENCES customers(id) ON DELETE SET NULL,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE SET NULL,
    FOREIGN KEY (supplier_id) REFERENCES suppliers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "supplier_payments" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "supplier_id" INTEGER NOT NULL,
    "amount" REAL NOT NULL DEFAULT 0,
    "payment_date" TEXT NOT NULL,
    "reference_no" TEXT,
    "payment_type" TEXT DEFAULT 'Cash',
    "notes" TEXT,
    "purchase_order_id" INTEGER,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (purchase_order_id) REFERENCES purchase_orders(id) ON DELETE SET NULL,
    FOREIGN KEY (supplier_id) REFERENCES suppliers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "suppliers" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "contact_person" TEXT,
    "phone" TEXT,
    "email" TEXT,
    "address" TEXT,
    "company_name" TEXT,
    "tax_number" TEXT,
    "website" TEXT,
    "payment_terms" TEXT,
    "bank_account" TEXT,
    "status" TEXT DEFAULT 'Active',
    "created_at" TEXT DEFAULT (kay_clock_timestamp())
);

CREATE TABLE IF NOT EXISTS "user_activity_log" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "user_id" INTEGER,
    "username" TEXT NOT NULL,
    "action" TEXT NOT NULL,
    "details" TEXT,
    "ip_address" TEXT,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "user_roles" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "permissions" TEXT,
    "is_system" INTEGER DEFAULT 0,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS "users" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "username" TEXT NOT NULL,
    "password_hash" TEXT NOT NULL,
    "role" TEXT NOT NULL DEFAULT 'Cashier',
    "full_name" TEXT,
    "salt" TEXT,
    "force_password_change" INTEGER DEFAULT 0,
    "permissions" TEXT,
    "last_login" TEXT,
    "is_active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (username)
);

CREATE TABLE IF NOT EXISTS "variant_batch_changes" (
    "movement_id" INTEGER NOT NULL,
    "product_id" INTEGER NOT NULL,
    "variant_id" INTEGER NOT NULL,
    "location" TEXT NOT NULL,
    "batch_no" TEXT NOT NULL,
    "expire_date" TEXT NOT NULL,
    "delta" INTEGER NOT NULL,
    "expiry_unknown" INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (movement_id) REFERENCES stock_movements(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "variant_stock_batches" (
    "product_id" INTEGER NOT NULL,
    "variant_id" INTEGER NOT NULL,
    "location" TEXT NOT NULL,
    "batch_no" TEXT NOT NULL,
    "expire_date" TEXT NOT NULL DEFAULT '',
    "quantity" INTEGER NOT NULL DEFAULT 0,
    "expiry_unknown" INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, variant_id, location, batch_no, expire_date),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
    FOREIGN KEY (variant_id) REFERENCES product_variants(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "zkteco_attendance_logs" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "device_id" INTEGER NOT NULL,
    "device_user_id" TEXT NOT NULL,
    "employee_id" INTEGER,
    "punch_time" TEXT NOT NULL,
    "status" INTEGER,
    "punch" INTEGER,
    "verification_type" INTEGER,
    "imported_at" TEXT DEFAULT (kay_clock_timestamp()),
    "is_valid" INTEGER DEFAULT 1,
    "validation_note" TEXT,
    UNIQUE (device_id, device_user_id, punch_time, punch),
    FOREIGN KEY (device_id) REFERENCES zkteco_devices(id) ON DELETE CASCADE,
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "zkteco_devices" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "device_no" INTEGER NOT NULL,
    "name" TEXT,
    "ip_address" TEXT NOT NULL,
    "port" INTEGER DEFAULT 4370,
    "comm_key" INTEGER DEFAULT 0,
    "serial_no" TEXT,
    "last_sync_at" TEXT,
    "is_active" INTEGER DEFAULT 1,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (device_no)
);

CREATE TABLE IF NOT EXISTS "zkteco_employee_mappings" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "device_id" INTEGER,
    "employee_id" INTEGER,
    "device_user_id" TEXT NOT NULL,
    "created_at" TEXT DEFAULT (kay_clock_timestamp()),
    UNIQUE (device_id, device_user_id),
    UNIQUE (device_id, employee_id),
    FOREIGN KEY (device_id) REFERENCES zkteco_devices(id) ON DELETE CASCADE,
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS kay_schema_versions(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS rust_sequences(name TEXT PRIMARY KEY,value INTEGER NOT NULL DEFAULT 0);

INSERT OR IGNORE INTO rust_sequences(name) VALUES('rust_invoice_number_seq'),('rust_batch_number_seq');

-- Rust modules with additive schemas not present in older server installations.
CREATE TABLE IF NOT EXISTS rust_management_requests (
 request_id TEXT PRIMARY KEY,username TEXT NOT NULL,payload_hash TEXT NOT NULL,result_id INTEGER NOT NULL,
 created_at TEXT NOT NULL DEFAULT (kay_clock_timestamp())
);
CREATE TABLE IF NOT EXISTS rust_expense_files (
 attachment_id INTEGER PRIMARY KEY REFERENCES expense_attachments(id) ON DELETE CASCADE,
 content BLOB NOT NULL CHECK(length(content) BETWEEN 1 AND 10485760),sha256 TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS kay_sales_date ON sales(created_at,status);
CREATE INDEX IF NOT EXISTS kay_sale_items_sale ON sale_items(sale_id);
CREATE INDEX IF NOT EXISTS kay_credit_customer ON credit_sales(customer_id,status,balance_amount);
CREATE INDEX IF NOT EXISTS kay_credit_payments_customer ON credit_payments(customer_id,payment_date);
CREATE INDEX IF NOT EXISTS kay_products_category ON products(category_id);
CREATE INDEX IF NOT EXISTS kay_products_barcode ON products(barcode);
CREATE INDEX IF NOT EXISTS kay_products_sku ON products(sku);
CREATE INDEX IF NOT EXISTS kay_activity_date ON user_activity_log(created_at,id);

CREATE UNIQUE INDEX IF NOT EXISTS expense_budgets_period ON expense_budgets(category,month,year);
