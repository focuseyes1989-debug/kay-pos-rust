import json,re,psycopg
from pathlib import Path
root=Path(__file__).resolve().parents[1]
config=json.loads((root/'kay-pos-db.json').read_text(encoding='utf-8-sig'))
source=(root/'crates/pos_core/src/ai.rs').read_text(encoding='utf-8')
sql=re.search(r'const PAYMENT_SALES_SQL: &str = r"(.*?)";',source,re.S).group(1)
with psycopg.connect(host=config['host'],port=config['port'],dbname=config['database'],user=config['username'],password=config['password'],connect_timeout=5) as conn:
 conn.execute('SET TRANSACTION READ ONLY')
 conn.execute("SET LOCAL statement_timeout='10s'")
 for pattern in [None,r'^(kpay|kbzpay)([(][0-9]+[)])?$']:
  values={'1':'2026-10-06','2':'2026-10-07','3':pattern}
  params=tuple(values[n] for n in re.findall(r'\$([123])',sql))
  rows=conn.execute(re.sub(r'\$[123]','%s',sql),params).fetchall()
  if pattern: assert all(row[0].startswith('KPay') for row in rows)
  print('PASS payment sales query:',len(rows),'groups')
 # Synthetic aliases, nulls and statuses exercise the actual production aggregation.
 fixture=sql.replace("FROM sales WHERE created_at >= $1 AND created_at < $2 AND status='completed'","FROM (VALUES (10::numeric,'KPay','completed'),(20,'KBZ Pay','completed'),(30,'KPay(1)','completed'),(99,'KPay','refunded'),(5,'Cash','completed'),(7,NULL,'completed')) AS sales(total,payment_type,status) WHERE status='completed'")
 for pattern in [None,r'^(kpay|kbzpay)([(][0-9]+[)])?$']:
  params=tuple(pattern for n in re.findall(r'\$3',fixture))
  rows=conn.execute(fixture.replace('$3','%s'),params).fetchall()
  amounts={row[0]:(row[1],row[2]) for row in rows}
  assert amounts['KPay']==(2,'30')
  assert amounts['KPay(1)']==(1,'30')
  assert len(rows)==(2 if pattern else 4)
 exact=fixture.replace('$3','%s')
 rows=conn.execute(exact,(r'^(kpay|kbzpay)[(]1[)]$',)*2).fetchall()
 assert rows==[('KPay(1)',1,'30')]
 print('PASS account-specific filter, alias aggregation and refunded exclusion')
 conn.rollback()
