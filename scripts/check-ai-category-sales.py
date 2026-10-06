"""Read-only checks of category sales aggregation and category resolution."""
import json
import re
from decimal import Decimal
from pathlib import Path

import psycopg

root = Path(__file__).resolve().parents[1]
config = json.loads((root / 'kay-pos-db.json').read_text(encoding='utf-8-sig'))
source = (root / 'crates/pos_core/src/ai.rs').read_text(encoding='utf-8')
sql = re.search(r'const CATEGORY_SALES_SQL: &str = "([^"]+)"', source).group(1)
resolve = re.search(r'const CATEGORY_LOOKUP_SQL: &str = "([^"]+)"', source).group(1)
partial = re.search(r'let partial:bool=sqlx::query_scalar\("(SELECT EXISTS[^"\n]+JOIN products[^"\n]+)"', source).group(1)

def pg(query):
    return re.sub(r'\$(\d+)', lambda m: '%(p' + m[1] + ')s', query)

fixtures = """sales(id,created_at,total,status) AS (VALUES
 (1,'2026-10-05 09:00'::timestamp,13500::numeric,'completed'),
 (2,'2026-10-05 10:00'::timestamp,1000::numeric,'completed'),
 (3,'2026-10-05 10:00'::timestamp,9000::numeric,'refunded'),
 (4,'2026-10-06 00:00'::timestamp,9000::numeric,'completed')),
 categories(id,name) AS (VALUES (1,'Services'),(2,'General')),
 products(id,category_id) AS (VALUES (1,1),(2,2)),
 sale_items(sale_id,product_id,qty,total,refunded_qty) AS (VALUES
 (1,1,1::numeric,10000::numeric,0),(1,2,2,5000,0),
 (2,1,1,2000,0),(3,1,1,9000,0),(4,1,1,9000,0)), """
params = {'p1': '2026-10-05 00:00:00', 'p2': '2026-10-06 00:00:00', 'p3': 'Services'}
with psycopg.connect(host=config['host'], port=config['port'], dbname=config['database'],
                     user=config['username'], password=config['password']) as conn:
    conn.execute('SET TRANSACTION READ ONLY')
    conn.execute("SET LOCAL statement_timeout='10s'")
    fixture_sql = pg(sql.replace('WITH lines AS', 'WITH ' + fixtures + 'lines AS', 1))
    rows = conn.execute(fixture_sql, params).fetchall()
    assert rows == [('Services', 2, '10000.00', '2', '12000')], rows
    all_rows = conn.execute(fixture_sql, dict(params, p3=None)).fetchall()
    assert sum(Decimal(row[2]) for row in all_rows) == Decimal('14500')
    assert not conn.execute(fixture_sql, dict(params, p3='Nonexistent')).fetchall()
    fixture_partial = pg('WITH ' + fixtures.rstrip(', ') + ' ' + partial)
    assert not conn.execute(fixture_partial, params).fetchone()[0]
    assert conn.execute(fixture_partial.replace('(2,1,1,2000,0)', '(2,1,1,2000,1)'), params).fetchone()[0]
    assert conn.execute(fixture_partial.replace('(1,2,2,5000,0)', '(1,2,2,5000,1)'), params).fetchone()[0]
    print('PASS fixtures: mixed categories, proportional receipt totals, receipt counts, refund/date exclusion, partial-refund detection')
    category = conn.execute(pg(resolve), {'p1': 'service'}).fetchone()
    assert category and category[0] == 'Services', category
    assert not conn.execute(pg(resolve), {'p1': "unknown_' OR 1=1"}).fetchone()
    ice = '\u101b\u1031\u1001\u1032\u1019\u102f\u1014\u103a\u1037'
    alternative = ice.replace('\u103a\u1037', '\u1037\u103a')
    for name in [ice, alternative]:
        normalized = name.replace('\u1037\u103a', '\u103a\u1037')
        resolved = conn.execute(pg(resolve), {'p1': normalized}).fetchone()
        assert resolved and resolved[0] == ice
        conn.execute(pg(sql), dict(params, p3=resolved[0])).fetchall()
    print('PASS Burmese category names: both tone-mark orders resolve to the same category')
    actual = conn.execute(pg(sql), dict(params, p3=category[0])).fetchall()
    assert all(row[0] == category[0] for row in actual)
    conn.execute(pg(partial), dict(params, p3=category[0])).fetchone()
    print(f'PASS database: Service resolves to Services; {len(actual)} category result rows')
    conn.rollback()
