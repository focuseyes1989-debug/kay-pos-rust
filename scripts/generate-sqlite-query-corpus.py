"""Generate structure-only SQL preparation cases from Rust source; never reads a shop database."""
import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRINGS = r'r(?P<h>#{0,8})"(?P<raw>[\s\S]*?)"(?P=h)|"(?P<quoted>(?:\\.|[^"\\])*)"'
TOKENS = re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|' + STRINGS + r'|[{}]')

def test_ranges(source):
    ranges = []
    for match in re.finditer(r'#\[cfg\(test\)\]\s*mod\s+\w+\s*\{', source):
        depth = 1
        for token in TOKENS.finditer(source, match.end()):
            if token.group() == '{':
                depth += 1
            elif token.group() == '}':
                depth -= 1
                if depth == 0:
                    ranges.append((match.start(), token.end()))
                    break
    return ranges

queries = []
for path in sorted((ROOT / 'crates/pos_core/src').glob('*.rs')):
    if path.name in ('storage.rs', 'sqlite.rs', 'sqlite_dialect.rs', 'compatibility.rs', 'data_transfer.rs') or path.name.startswith('erp_'):
        continue
    source = path.read_text(encoding='utf-8')
    excluded = test_ranges(source)
    for match in re.finditer(STRINGS, source):
        if any(a <= match.start() < b for a, b in excluded):
            continue
        if match.group('raw') is not None:
            sql = match.group('raw')
        else:
            try:
                sql = json.loads('"' + match.group('quoted') + '"')
            except ValueError:
                continue
        if not re.match(r'^(SELECT |WITH |INSERT INTO |UPDATE [a-z_]|DELETE FROM |SET (LOCAL|TRANSACTION) |LOCK TABLE )', sql.strip()):
            continue
        if '{' in sql or '...' in sql or sql in ('SELECT category,COUNT', 'SELECT item,COUNT', 'SELECT category, COUNT', 'SELECT item, COUNT'):
            continue
        if path.name == 'sale_summary.rs' and sql.lstrip().startswith('WITH window_sales'):
            sql += ' SELECT * FROM lines'
        queries.append({'source': path.name + ':' + str(source[:match.start()].count('\n') + 1), 'sql': sql})
for path in sorted((ROOT / 'crates/pos_core/src').glob('*.sql')):
    queries.append({'source': path.name, 'sql': path.read_text(encoding='utf-8')})
output = json.dumps(queries, indent=2, ensure_ascii=False) + '\n'
target = ROOT / 'crates/pos_core/tests/fixtures/sqlite_queries.json'
parser = argparse.ArgumentParser()
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
if args.check:
    if target.read_text(encoding='utf-8') != output:
        raise SystemExit('SQL corpus is stale; run scripts/generate-sqlite-query-corpus.py')
else:
    target.write_text(output, encoding='utf-8')
print(f'{len(queries)} standalone preparation cases')
