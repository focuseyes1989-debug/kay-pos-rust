WITH period_sales AS (
    SELECT * FROM sales WHERE created_at >= $1::text::timestamp AND created_at < $2::text::timestamp
)
SELECT 'Completed sales'::text AS label,COALESCE(SUM(total),0)::float8 AS amount,COUNT(*) AS count FROM period_sales WHERE status='completed'
UNION ALL SELECT 'Received at checkout',COALESCE(SUM(GREATEST(0,COALESCE(payment,0)-COALESCE(change_amount,0))),0)::float8,COUNT(*) FROM period_sales WHERE status='completed'
UNION ALL SELECT 'Credit sales',COALESCE(SUM(total),0)::float8,COUNT(*) FROM period_sales WHERE status='completed' AND lower(trim(payment_type))='credit'
UNION ALL SELECT 'Discounts',COALESCE(SUM(discount_amount),0)::float8,COUNT(*) FROM period_sales WHERE status='completed' AND discount_amount>0
UNION ALL SELECT 'Refunded sales (sale date)',COALESCE(SUM(total),0)::float8,COUNT(*) FROM period_sales WHERE status='refunded'
UNION ALL SELECT 'Credit collections',COALESCE(SUM(amount),0)::float8,COUNT(*) FROM credit_payments WHERE left(payment_date,10)>=$1 AND left(payment_date,10)<$2
UNION ALL SELECT 'Recorded expenses',COALESCE(SUM(amount),0)::float8,COUNT(*) FROM expenses WHERE left(expense_date,10)>=$1 AND left(expense_date,10)<$2
UNION ALL SELECT 'Supplier payments',COALESCE(SUM(amount),0)::float8,COUNT(*) FROM supplier_payments WHERE payment_type<>'Purchase' AND left(payment_date,10)>=$1 AND left(payment_date,10)<$2
UNION ALL SELECT 'Supplier purchases (ledger)',COALESCE(SUM(amount),0)::float8,COUNT(*) FROM supplier_payments WHERE payment_type='Purchase' AND left(payment_date,10)>=$1 AND left(payment_date,10)<$2
