import json,re,psycopg
from pathlib import Path
root=Path(__file__).resolve().parents[1]
config=json.loads((root/'kay-pos-db.json').read_text(encoding='utf-8-sig'))
source=(root/'crates/pos_core/src/ai.rs').read_text(encoding='utf-8')
inventory=re.search(r'"(WITH stock AS [^"\n]+)"',source).group(1)
expense=re.search(r'"(SELECT json_build_array\(COALESCE\(category[^"\n]+)"',source).group(1)
products=re.search(r'"(SELECT p.name,COALESCE\(p.sku[^"\n]+)"',source).group(1)
stock_filter=re.search(r'const INVENTORY_SEARCH_FILTER:&str="([^\"\n]+)"',source).group(1)
cases=[('inventory-all',inventory.replace('quantity<=threshold','TRUE'),('2026-10-01','2026-10-06')),
 ('inventory-low',inventory,('2026-10-01','2026-10-06')),
 ('inventory-out',inventory.replace('quantity<=threshold','quantity<=0'),('2026-10-01','2026-10-06')),
 ('inventory-search',inventory.replace('quantity<=threshold',"strpos(lower(name||' '||COALESCE(sku,'')||' '||COALESCE(barcode,'')),lower($3))>0"),('2026-10-01','2026-10-06','milk')),
 ('expenses',expense,('2026-10-01','2026-10-06')),
 ('expenses-search',expense.replace('GROUP BY category',"AND strpos(lower(COALESCE(category,'')||' '||COALESCE(description,'')),lower($3))>0 GROUP BY category"),('2026-10-01','2026-10-06','fuel')),
 ('products-all',products,('',)),('products-search',products,('milk',)),('products-literal',products,("xyz_unique_' OR 1=1 --",))]
with psycopg.connect(host=config['host'],port=config['port'],dbname=config['database'],user=config['username'],password=config['password']) as conn:
 conn.execute('SET TRANSACTION READ ONLY')
 conn.execute("SET LOCAL statement_timeout='10s'")
 # Actual category filter: bind every occurrence of $3 for psycopg's positional placeholders.
 for term in ['ရေခဲမုန့်','ရေခဲမုန့်','unmatched_stock_category']:
  sql=inventory.replace('quantity<=threshold',stock_filter)
  values={'1':'2026-10-01','2':'2026-10-06','3':term}
  params=tuple(values[n] for n in re.findall(r'\$([123])',sql))
  rows=conn.execute(re.sub(r'\$[123]','%s',sql),params).fetchall()
  if term.startswith('unmatched'):assert not rows
  print('PASS inventory category search:',len(rows),'rows')
 for name,sql,params in cases:
  rows=conn.execute(re.sub(r'\$[123]','%s',sql),params).fetchall()
  assert len(rows)<=51
  if name=='products-literal':assert not rows
  print(f'PASS {name}: {len(rows)} rows')
 conn.rollback()
