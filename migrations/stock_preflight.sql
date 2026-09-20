-- Read-only. Run before rollout; review results in Main POS, do not auto-repair.
SELECT p.id, p.name, p.stock AS master_stock, COALESCE(b.quantity,0) AS batch_stock
FROM products p
LEFT JOIN (SELECT product_id,SUM(quantity) AS quantity FROM product_locations GROUP BY product_id) b ON b.product_id=p.id
WHERE LOWER(COALESCE(p.sold_by,'Each')) NOT IN ('service','restaurant','variants')
  AND ABS(COALESCE(p.stock,0)-COALESCE(b.quantity,0)) > 0.000001;

SELECT v.id AS variant_id,v.product_id,v.stock AS variant_stock,COALESCE(b.quantity,0) AS batch_stock
FROM product_variants v
LEFT JOIN (SELECT variant_id,SUM(quantity) AS quantity FROM variant_stock_batches GROUP BY variant_id) b ON b.variant_id=v.id
WHERE ABS(COALESCE(v.stock,0)-COALESCE(b.quantity,0)) > 0.000001;

SELECT p.id,p.name,p.stock AS master_stock,COALESCE(v.quantity,0) AS variant_stock
FROM products p
LEFT JOIN (SELECT product_id,SUM(stock) AS quantity FROM product_variants GROUP BY product_id) v ON v.product_id=p.id
WHERE LOWER(COALESCE(p.sold_by,''))='variants'
  AND ABS(COALESCE(p.stock,0)-COALESCE(v.quantity,0)) > 0.000001;
