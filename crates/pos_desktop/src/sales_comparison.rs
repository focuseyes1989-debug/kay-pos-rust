use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{sales_comparison,models::Money};
use crate::expense_comparison::{preset,percentage,amount};
use std::collections::BTreeMap;
fn sales_percentage(a:Money,b:Money)->String{percentage(a,b).replace("New spending","New sales")}
#[component]
pub fn SalesComparison(db_form:DbForm,actor:pos_core::auth::Session)->Element{
 let mut kind=use_signal(||0usize);
 let mut dates=use_signal(||preset(chrono::Local::now().date_naive(),0));
 let mut dimension=use_signal(||0usize);
 let mut data=use_resource(move||{let form=db_form.clone();let actor=actor.clone();let (a,b,c,d)=dates();async move{async{
  let pool=pos_core::connect(&form.database_config()?).await?;
  sales_comparison::load(&pool,&actor,&a,&b,&c,&d).await
 }.await.map_err(|e|format!("{e:#}"))}});
 let (a,b,c,d)=dates();
 let invalid=a>b||c>d;
 let state=data.read();
 let mut totals=BTreeMap::<String,(Money,Money)>::new();
 if let Some(Ok((current,previous)))=state.as_ref(){
  let first=if dimension()==0{&current.categories}else{&current.products};
  let second=if dimension()==0{&previous.categories}else{&previous.products};
  for (name,value) in first{totals.entry(name.clone()).or_default().0=*value;}
  for (name,value) in second{totals.entry(name.clone()).or_default().1=*value;}
 }
 let current:Money=totals.values().map(|v|v.0).sum();let previous:Money=totals.values().map(|v|v.1).sum();
 let mut changes=totals.iter().map(|(n,(x,y))|(n.clone(),*x-*y,*x,*y)).collect::<Vec<_>>();
 changes.sort_by(|a,b|a.1.cmp(&b.1).then_with(||a.0.cmp(&b.0)));
 rsx!{section{class:"expense_comparison",
  button{hidden:true,"data-page-refresh":"true",onclick:move |_|data.restart()}
  div{class:"expense_comparison_filters",
   label{"Compare" select{value:kind().to_string(),onchange:move|e|{let value=e.value().parse().unwrap_or(0);kind.set(value);if value<2{dates.set(preset(chrono::Local::now().date_naive(),value));}},
    option{value:"0","Current Month vs Last Month"}option{value:"1","Current Month vs Same Month Last Year"}option{value:"2","Custom Period Comparison"}
   }}
   div{class:"comparison_period",span{"Current period"} crate::date_range::DateRangePicker{from:a.clone(),to:b.clone(),disabled:kind()!=2,onchange:move|(a,b)|{let (_,_,c,d)=dates();dates.set((a,b,c,d));}}}
   div{class:"comparison_period",span{"Previous period"} crate::date_range::DateRangePicker{from:c.clone(),to:d.clone(),disabled:kind()!=2,onchange:move|(c,d)|{let (a,b,_,_)=dates();dates.set((a,b,c,d));}}}
  }
  if invalid{p{role:"alert","From date must not be after To date."}}
  else if !data.finished(){p{role:"status","Loading comparison..."}}
  else if let Some(Err(error))=state.as_ref(){p{role:"alert","{error}"}}
  else{
   div{class:"comparison_totals",
    article{span{class:"comparison_label","Current period"}p{class:"comparison_amount","{amount(current)}"}small{"{a} – {b}"}}
    article{span{class:"comparison_label","Previous period"}p{class:"comparison_amount","{amount(previous)}"}small{"{c} – {d}"}}
    article{class:if current<previous{"comparison_difference decreased"}else if current>previous{"comparison_difference increased"}else{"comparison_difference unchanged"},
     span{class:"comparison_label",if current<previous{"↓ Sales decreased"}else if current>previous{"↑ Sales increased"}else{"→ Sales unchanged"}}
     p{class:"comparison_amount","{amount((current-previous).abs())}"}
     small{"{sales_percentage(current,previous)} compared with the previous period"}
    }
   }
   if let Some(Ok((first,second)))=state.as_ref(){
    div{class:"comparison_totals",
     article{span{"Quantity sold"}p{class:"comparison_amount","{first.quantity} / {second.quantity}"}small{"Current / previous"}}
     article{span{"Receipts"}p{class:"comparison_amount","{first.receipts} / {second.receipts}"}small{"Current / previous"}}
    }
   }
   div{class:"receipt_tabs",role:"tablist",for (index,label) in ["Categories","Products"].iter().enumerate(){button{role:"tab",aria_selected:dimension()==index,class:if dimension()==index{"active"}else{""},onclick:move |_|dimension.set(index),"{label}"}}}
   div{class:"expense_comparison_highlights",
    for (title,increase) in [("Largest sales decreases",false),("Largest sales increases",true)]{
     section{class:if increase{"increased"}else{"decreased"},h3{"{title}"}p{class:"comparison_section_hint","Top 5 categories by change in amount"}
      if !changes.iter().any(|v|if increase{v.1>Money::ZERO}else{v.1<Money::ZERO}){p{"No sales changes in this direction."}}
      for row in if increase{changes.iter().rev().filter(|v|v.1>Money::ZERO).take(5).collect::<Vec<_>>()}else{changes.iter().filter(|v|v.1<Money::ZERO).take(5).collect::<Vec<_>>()}{
       div{class:"expense_comparison_change",span{class:"comparison_category_name","{row.0}"}span{"{amount(row.1)}"}small{"{sales_percentage(row.2,row.3)}"}}
      }
     }
    }
   }
   section{class:"comparison_details",header{h3{"Sales by category / product"}p{"Current and previous amounts shown side by side. New sales means the previous amount was zero."}}
   div{class:"customers_table_scroll",table{class:"customers_table",thead{tr{for label in ["Category","Current Period","Previous Period","Difference","Change %","Trend"]{th{"{label}"}}}}tbody{
    for (name,(x,y)) in &totals{tr{td{"{name}"}td{"{amount(*x)}"}td{"{amount(*y)}"}td{"{amount(*x-*y)}"}td{"{sales_percentage(*x,*y)}"}td{span{class:if x>y{"comparison_trend increased"}else if x<y{"comparison_trend decreased"}else{"comparison_trend unchanged"},if x>y{"↑ Increased"}else if x<y{"↓ Decreased"}else{"→ No change"}}}}}
   }}}
   }
  }
 }}
}
