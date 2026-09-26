-- Main POS-compatible tables. Apply after a verified backup, as the schema owner.
CREATE TABLE IF NOT EXISTS user_activity_log (
    id SERIAL PRIMARY KEY, user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    username TEXT NOT NULL, action TEXT NOT NULL, details TEXT, ip_address TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_rust_activity_created ON user_activity_log(created_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS category_groups (
    id SERIAL PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT,
    sort_order INTEGER DEFAULT 0, icon TEXT DEFAULT '', color TEXT DEFAULT '#6c5ce7',
    is_favorite INTEGER DEFAULT 0, is_active INTEGER DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
ALTER TABLE categories ADD COLUMN IF NOT EXISTS group_id INTEGER REFERENCES category_groups(id) ON DELETE SET NULL;
