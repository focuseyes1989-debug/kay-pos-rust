import json, re, psycopg
from pathlib import Path
root=Path(__file__).resolve().parents[1]
config=json.loads((root/'kay-pos-db.json').read_text(encoding='utf-8-sig'))
source=(root/'crates/pos_core/src/assistant.rs').read_text(encoding='utf-8')
queries=re.findall(r'"(SELECT json_build_array[^"\n]+)"',source)
with psycopg.connect(host=config['host'],port=config['port'],dbname=config['database'],user=config['username'],password=config['password']) as conn:
    conn.execute('SET TRANSACTION READ ONLY')
    conn.execute("SET LOCAL statement_timeout='10s'")
    for sql in queries:
        params=('unmatched_detail_key','') if '$4' not in sql else ('unmatched_detail_key','2026-10-01','2026-10-05','')
        rows=conn.execute(re.sub(r'\$[1-4]','%s',sql),params).fetchall()
        assert not rows
    conn.rollback()
print(f'PASS: {len(queries)} bound detail queries in read-only transaction; literal keys return no unrelated records')
