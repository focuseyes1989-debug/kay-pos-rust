"""Validate chat receipt SQL using read-only fixtures and the configured database."""
import json
import re
from decimal import Decimal
from pathlib import Path

import psycopg

root = Path(__file__).resolve().parents[1]
config = json.loads((root / 'kay-pos-db.json').read_text(encoding='utf-8-sig'))
source = (root / 'crates/pos_core/src/ai.rs').read_text(encoding='utf-8')
base = re.search(r'let mut sql="(SELECT id,COALESCE\(invoice_no[^"\n]+)"', source).group(1)
predicate = re.search(r'if amount.is_some\(\) \|\| category.is_some\(\) \{sql.push_str\("([^"]+)"\)', source).group(1)
def pg(sql):
    return re.sub(r'\$(\d+)', lambda m: '%(p' + m[1] + ')s', sql)

category_filter = re.search(r'const RECEIPT_CATEGORY_SQL: &str = "([^"]+)"', source).group(1)
order = ' ORDER BY created_at DESC,id DESC LIMIT 51'
sql = pg(base + predicate + order)
category_sql = pg(base + predicate + category_filter + order)
fixture = """WITH sales(id,invoice_no,created_at,total,payment_type,status) AS (VALUES
 (1,'A','2026-10-05 00:00:00'::timestamp,10000::numeric,'Cash','completed'),
 (2,'B','2026-10-05 23:59:59'::timestamp,10000::numeric,'Credit','completed'),
 (3,'C','2026-10-05 12:00:00'::timestamp,9999::numeric,'Cash','completed'),
 (4,'D','2026-10-06 00:00:00'::timestamp,10000::numeric,'Cash','completed'),
 (5,'E','2026-10-05 12:00:00'::timestamp,10000::numeric,'Cash','refunded'),
 (6,'F','2026-10-04 23:59:59'::timestamp,10000::numeric,'Cash','completed'),
 (7,'G','2026-10-05 10:00:00'::timestamp,10000.50::numeric,'Cash','completed')) """
with psycopg.connect(host=config['host'], port=config['port'], dbname=config['database'],
                     user=config['username'], password=config['password']) as conn:
    conn.execute('SET TRANSACTION READ ONLY')
    conn.execute("SET LOCAL statement_timeout='10s'")
    params = {'p1': '2026-10-05', 'p2': '2026-10-06', 'p3': Decimal('10000')}
    rows = conn.execute(fixture + sql, params).fetchall()
    assert [row[0] for row in rows] == [2, 1]
    decimal_rows = conn.execute(fixture + sql, dict(params, p3=Decimal('10000.50'))).fetchall()
    assert [row[0] for row in decimal_rows] == [7]
    assert not conn.execute(fixture + sql, dict(params, p3=Decimal('20000'))).fetchall()
    print('PASS fixtures: exact total, day boundaries, completed status, decimals, no match')
    category_fixture = fixture.rstrip() + """, categories(id,name) AS (VALUES (1,'Ice cream'),(2,'General')),
 products(id,category_id) AS (VALUES (1,1),(2,2)),
 sale_items(sale_id,product_id,qty,refunded_qty) AS (VALUES
 (1,1,2,0),(1,1,1,0),(1,2,1,0),(2,2,1,0),(3,1,1,0),
 (4,1,1,0),(5,1,1,0),(6,1,1,0),(7,1,1,1)) """
    cat_params = dict(params, p3=None, p4='Ice cream')
    category_rows = conn.execute(category_fixture + category_sql, cat_params).fetchall()
    assert [row[0] for row in category_rows] == [3, 1]
    assert [row[0] for row in conn.execute(category_fixture + category_sql, dict(cat_params, p3=Decimal('10000'))).fetchall()] == [1]
    assert not conn.execute(category_fixture + category_sql, dict(cat_params, p4="unknown_' OR 1=1")).fetchall()
    print('PASS category fixtures: matching categories only, no duplicate receipts, fully returned items excluded, amount/category combination')
    payment_filter = re.search(r'const RECEIPT_PAYMENT_SQL: &str = "([^"]+)"', source).group(1)
    pattern = re.search(r'"KPay"=>"([^"]+)"', source).group(1)
    payment_sql = pg(base + predicate + ' AND ($4::text IS NULL)' + payment_filter + order)
    payment_fixture = fixture.replace("10000::numeric,'Cash','completed'", "10000::numeric,'Kpay (510)','completed'").replace("10000::numeric,'Credit','completed'", "10000::numeric,'Kpay(889)','completed'").replace("10000.50::numeric,'Cash','completed'", "10000.50::numeric,'KBZPay','completed'")
    payment_params = dict(params, p3=None, p4=None, p5=pattern)
    assert [row[0] for row in conn.execute(payment_fixture + payment_sql, payment_params).fetchall()] == [2, 7, 1]
    assert [row[0] for row in conn.execute(payment_fixture + payment_sql, dict(payment_params, p3=Decimal('10000'))).fetchall()] == [2, 1]
    combined_sql = pg(base + predicate + category_filter + payment_filter + order)
    combined_fixture = category_fixture.replace("10000::numeric,'Cash','completed'", "10000::numeric,'Kpay (510)','completed'")
    assert [row[0] for row in conn.execute(combined_fixture + combined_sql, dict(payment_params, p3=Decimal('10000'), p4='Ice cream')).fetchall()] == [1]
    print('PASS payment fixtures: KPay account suffixes, KBZPay alias, exact amount, combined category/payment filters')
    actual_payment = conn.execute(payment_sql, payment_params).fetchall()
    assert all(re.fullmatch(pattern, re.sub(r'\s', '', row[4].lower())) and row[5] == 'completed' for row in actual_payment)
    print(f'PASS database: {len(actual_payment)} KPay receipts (up to 51)')
    actual = conn.execute(sql, params).fetchall()
    assert all(Decimal(row[3]) == Decimal('10000') and row[5] == 'completed' for row in actual)
    for row in actual:
        assert conn.execute('SELECT id FROM sales WHERE id=%s', (row[0],)).fetchone()
        conn.execute('SELECT qty,price,total FROM sale_items WHERE sale_id=%s', (row[0],)).fetchall()
    print(f'PASS database: {len(actual)} matching receipts and detail IDs')
    category = '\u101b\u1031\u1001\u1032\u1019\u102f\u1014\u103a\u1037'
    assert conn.execute('SELECT id FROM categories WHERE name=%s', (category,)).fetchone()
    actual_category = conn.execute(category_sql, dict(params, p3=None, p4=category)).fetchall()
    for row in actual_category:
        assert conn.execute('SELECT 1 FROM sale_items i JOIN products p ON p.id=i.product_id JOIN categories c ON c.id=p.category_id WHERE i.sale_id=%s AND c.name=%s AND i.qty>COALESCE(i.refunded_qty,0)', (row[0], category)).fetchone()
    print(f'PASS database: {len(actual_category)} ice cream category receipts')
    conn.rollback()
