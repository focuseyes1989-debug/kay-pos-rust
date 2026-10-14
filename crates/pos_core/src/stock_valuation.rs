use crate::Product;
use rust_decimal::{Decimal, prelude::FromPrimitive};

#[derive(Clone, Debug, PartialEq)]
pub struct StockValueRow {
    pub product_id: i32,
    pub name: String,
    pub variant: String,
    pub category: String,
    pub stock: f64,
    pub unit_cost: Option<Decimal>,
    pub value: Option<Decimal>,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct StockValuation {
    pub rows: Vec<StockValueRow>,
    pub total: Decimal,
    pub missing_cost_products: usize,
    pub missing_cost_in_stock: usize,
    pub excluded_services: usize,
    pub negative_stock_rows: usize,
}

/// Current on-hand value, not historical purchases or sales revenue.
/// Active variants replace the parent's aggregate stock; locations and batches
/// are allocations of that stock and must never be added a second time.
pub fn calculate(products: &[Product]) -> StockValuation {
    let mut result=StockValuation::default();
    let mut missing=std::collections::HashSet::new();
    let mut missing_stock=std::collections::HashSet::new();
    for p in products {
        let mode=p.sold_by.as_deref().unwrap_or("Each").trim().replace('_'," ").to_lowercase();
        if mode=="service" || mode=="services" || mode.ends_with(" service") {
            result.excluded_services+=1;
            continue;
        }
        let variants=mode=="variant" || mode=="variants" || mode.ends_with(" variants");
        let entries:Vec<(String,f64,f64)>=if variants && !p.variants.is_empty() {
            p.variants.iter().map(|v|(
                [v.size.as_deref(),v.color.as_deref(),v.sku.as_deref()].into_iter().flatten().filter(|s|!s.trim().is_empty()).collect::<Vec<_>>().join(" / "),v.stock,v.cost
            )).collect()
        } else {vec![(String::new(),p.stock,p.cost)]};
        for (variant,stock,cost) in entries {
            let unit_cost=Decimal::from_f64(cost).filter(|c|*c>Decimal::ZERO);
            let quantity=Decimal::from_f64(stock).map(|q|q.max(Decimal::ZERO));
            let value=unit_cost.zip(quantity).and_then(|(c,q)|c.checked_mul(q));
            if stock<0.0 {result.negative_stock_rows+=1;}
            if unit_cost.is_none() {
                missing.insert(p.id);
                if stock>0.0 {missing_stock.insert(p.id);}
            }
            if let Some(value)=value {result.total+=value;}
            result.rows.push(StockValueRow {product_id:p.id,name:p.name.clone(),variant,category:p.category_name.clone().unwrap_or_else(||"Uncategorized".into()),stock,unit_cost,value});
        }
    }
    result.missing_cost_products=missing.len();
    result.missing_cost_in_stock=missing_stock.len();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn product(id:i32,mode:&str,stock:f64,cost:f64)->Product {
        serde_json::from_value(serde_json::json!({"id":id,"name":format!("P{id}"),"category_id":null,"category_name":null,"sku":null,"barcode":null,"price":999,"cost":cost,"stock":stock,"low_stock":0,"sold_by":mode,"image_filename":null,"image_data_url":null,"variants":[],"price_tiers":[]})).unwrap()
    }
    #[test]
    fn excludes_services_missing_cost_and_negative_stock() {
        let result=calculate(&[product(1,"Each",2.5,100.1),product(2,"Service",100.0,900.0),product(3,"Each",5.0,0.0),product(4,"Each",0.0,0.0),product(5,"Each",-3.0,50.0)]);
        assert_eq!(result.total,Decimal::new(25025,2));
        assert_eq!(result.missing_cost_products,2);
        assert_eq!(result.missing_cost_in_stock,1);
        assert_eq!(result.excluded_services,1);
        assert_eq!(result.negative_stock_rows,1);
        assert!(result.rows.iter().find(|r|r.product_id==3).unwrap().value.is_none());
    }
    #[test]
    fn variant_costs_replace_parent_and_never_fall_back_to_parent_cost() {
        let mut p=product(1,"Variants",999.0,999.0);
        for (id,stock,cost) in [(1,3.0,25.0),(2,2.0,40.0),(3,5.0,0.0)] {
            p.variants.push(crate::ProductVariant {variant_id:id,product_id:1,size:Some(id.to_string()),color:None,sku:None,barcode:None,price:100.0,cost,stock,low_stock:0.0,wholesale_min_qty:0,wholesale_price:0.0});
        }
        let result=calculate(&[p]);
        assert_eq!(result.total,Decimal::from(155));
        assert_eq!(result.rows.len(),3);
        assert_eq!(result.missing_cost_products,1);
        assert_eq!(result.missing_cost_in_stock,1);
    }
}
