"""Read-only PostgreSQL fixtures for product-specific assistant sales."""
import json
import re
from pathlib import Path
import psycopg

root = Path(__file__).resolve().parents[1]
source = (root / "crates/pos_core/src/ai.rs").read_text(encoding="utf-8")
def query(name):
    sql = re.search(r'const ' + name + r': &str = "([^"\n]+)"', source).group(1)
    return re.sub(r'\$(\d+)', lambda m: '%(p' + m[1] + ')s', sql)
fixtures = """sales(id,created_at,total,status) AS (VALUES
 (1,'2026-10-09 09:00'::timestamp,13500::numeric,'completed'),
 (2,'2026-10-09 10:00'::timestamp,1000::numeric,'completed'),
 (3,'2026-10-09 11:00'::timestamp,9000::numeric,'refunded'),
 (4,'2026-10-10 00:00'::timestamp,9000::numeric,'completed')),
 products(id,name,sku,barcode) AS (VALUES
 (1,'Cremo Choco Crush Strawberry','STRAW','123'),(2,'Other product','OTHER','456')),
 sale_items(sale_id,product_id,qty,total,refunded_qty) AS (VALUES
 (1,1,1::numeric,10000::numeric,0),(1,2,2,5000,0),
 (2,1,1,2000,0),(3,1,1,9000,0),(4,1,1,9000,0))"""
config = json.loads((root / "kay-pos-db.json").read_text(encoding="utf-8-sig"))
with psycopg.connect(host=config['host'],port=config['port'],dbname=config['database'],
                      user=config['username'],password=config['password'],connect_timeout=10) as conn:
    conn.execute('SET TRANSACTION READ ONLY')
    conn.execute("SET LOCAL statement_timeout='10s'")
    sql = query('PRODUCT_SALES_SQL').replace('WITH lines AS', 'WITH ' + fixtures + ', lines AS', 1)
    params = dict(p1='2026-10-09 00:00:00',p2='2026-10-10 00:00:00',p3='1')
    rows = conn.execute(sql,params).fetchall()
    assert rows == [('Cremo Choco Crush Strawberry',2,'10000.00','2','12000')], rows
    assert conn.execute(sql,dict(params,p3='999')).fetchall() == []
    partial = 'WITH ' + fixtures + ' ' + query('PRODUCT_SALES_PARTIAL_SQL')
    assert not conn.execute(partial,params).fetchone()[0]
    assert conn.execute(partial.replace('(1,2,2,5000,0)','(1,2,2,5000,1)'),params).fetchone()[0]
    assert conn.execute(partial.replace('(2,1,1,2000,0)','(2,1,1,2000,1)'),params).fetchone()[0]
    lookup = 'WITH ' + fixtures + ' ' + query('PRODUCT_SALES_LOOKUP_SQL')
    for identifier in ['cremo choco crush strawberry','STRAW','straw','123']:
        assert conn.execute(lookup,dict(p1=identifier)).fetchall() == [(1,'Cremo Choco Crush Strawberry')]
    assert conn.execute(lookup,dict(p1="unknown_' OR 1=1")).fetchall() == []
    assert len(conn.execute(lookup.replace("(2,'Other product','OTHER','456')","(2,'Cremo Choco Crush Strawberry','OTHER','456')"),dict(p1='cremo choco crush strawberry')).fetchall()) == 2
    # Validate the real schema without changing any records or auditing a user action.
    conn.execute('EXPLAIN ' + query('PRODUCT_SALES_SQL'),params).fetchall()
    conn.execute('EXPLAIN ' + query('PRODUCT_SALES_PARTIAL_SQL'),params).fetchall()
    conn.execute('EXPLAIN ' + query('PRODUCT_SALES_LOOKUP_SQL'),dict(p1='cremo choco crush strawberry')).fetchall()
    conn.rollback()
print('PASS: product-only totals, discounts, receipt counts, date/refund exclusion, mixed-receipt partial refunds, exact name/SKU/barcode, duplicate names and schema validation (read-only)')
