Vendored rustzk 1.1.0 (MIT), https://github.com/vkaylee/rustzk.
Local fix: choose exact 8/16/40-byte attendance formats before padded-format
heuristics. Otherwise 40-byte K20 records are mistakenly decoded as 16-byte
records, losing employee IDs and timestamps. Covered by regression tests.
