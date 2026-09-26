CREATE TABLE users(id SERIAL PRIMARY KEY,username TEXT UNIQUE,role TEXT,is_active INTEGER DEFAULT 1,password_hash TEXT,salt TEXT);
CREATE TABLE user_activity_log(id SERIAL PRIMARY KEY,user_id INTEGER REFERENCES users(id),username TEXT NOT NULL,action TEXT NOT NULL,details TEXT,ip_address TEXT,created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE service_orders(
 id SERIAL PRIMARY KEY,order_no TEXT UNIQUE NOT NULL,job_title TEXT,complaint TEXT,internal_notes TEXT,
 status TEXT NOT NULL DEFAULT 'received',received_at TIMESTAMP NOT NULL,expected_at TIMESTAMP,
 started_by TEXT,started_at TIMESTAMP,completed_by TEXT,completed_at TIMESTAMP,
 delivered_by TEXT,delivered_at TIMESTAMP,created_by TEXT NOT NULL,created_at TIMESTAMP NOT NULL,updated_at TIMESTAMP NOT NULL,
 customer_name TEXT,customer_phone TEXT,sale_id INTEGER,deposit_amount REAL DEFAULT 0,checkout_started_at TIMESTAMP
);
CREATE TABLE service_order_status_history(id SERIAL PRIMARY KEY,service_order_id INTEGER REFERENCES service_orders ON DELETE CASCADE,from_status TEXT,to_status TEXT NOT NULL,note TEXT,changed_by TEXT NOT NULL,changed_at TIMESTAMP NOT NULL);
CREATE TABLE service_order_notifications(id SERIAL PRIMARY KEY,service_order_id INTEGER REFERENCES service_orders ON DELETE CASCADE,event TEXT,channel TEXT,recipient TEXT,message TEXT,status TEXT,attempts INTEGER,created_at TIMESTAMP);
CREATE TABLE service_order_payments(id SERIAL PRIMARY KEY,service_order_id INTEGER REFERENCES service_orders ON DELETE CASCADE);
CREATE TABLE service_order_design_prompts(id SERIAL PRIMARY KEY,title TEXT UNIQUE NOT NULL,category TEXT NOT NULL DEFAULT '',prompt_text TEXT NOT NULL,image_data TEXT,image_name TEXT,sort_order INTEGER NOT NULL DEFAULT 0,active INTEGER NOT NULL DEFAULT 1,created_at TIMESTAMP NOT NULL,updated_at TIMESTAMP NOT NULL);
