WITH period AS (
    SELECT * FROM sales WHERE created_at >= $1 AND created_at < $2
      AND status IN ('completed','refunded')
), costs AS (
    SELECT si.sale_id,
      CASE WHEN COUNT(*) FILTER (WHERE si.cost IS NULL OR si.qty IS NULL)>0
        THEN NULL ELSE SUM(si.qty::numeric*si.cost::numeric) END AS cost,
      COUNT(*) FILTER (WHERE COALESCE(si.refunded_qty,0)>0) AS partial
    FROM sale_items si JOIN period s ON s.id=si.sale_id GROUP BY si.sale_id
)
SELECT COALESCE(s.invoice_no,'#'||s.id::text) AS invoice,
    to_char(s.created_at,'YYYY-MM-DD HH24:MI:SS') AS day,
    to_char(s.created_at,'YYYY-MM') AS month,
    COALESCE(c.name,'Walk-in Customer') AS customer,
    COALESCE(s.payment_type,'') AS method,s.status,
    COALESCE(s.total,0)::numeric AS total,COALESCE(s.discount_amount,0)::numeric AS discount,
    GREATEST(0,COALESCE(s.payment,0)::numeric-COALESCE(s.change_amount,0)::numeric) AS received,
    costs.cost,COALESCE(costs.partial,0)::bigint AS partial
FROM period s LEFT JOIN costs ON costs.sale_id=s.id
LEFT JOIN customers c ON c.id=s.customer_id ORDER BY s.created_at,s.id
