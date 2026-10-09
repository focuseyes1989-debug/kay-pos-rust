use crate::DbForm;
use dioxus::prelude::*;
use chrono::{Datelike,NaiveDate,Duration};
use pos_core::{expenses,models::Money};
use std::collections::BTreeMap;
fn month(year:i32,month:u32)->(String,String){
 let start=NaiveDate::from_ymd_opt(year,month,1).unwrap();
 let next=if month==12{NaiveDate::from_ymd_opt(year+1,1,1)}else{NaiveDate::from_ymd_opt(year,month+1,1)}.unwrap();
 (start.to_string(),(next-Duration::days(1)).to_string())
}
pub(crate) fn preset(today:NaiveDate,kind:usize)->(String,String,String,String){
 let (a,b)=month(today.year(),today.month());
 let (c,d)=if kind==1{month(today.year()-1,today.month())}else if today.month()==1{month(today.year()-1,12)}else{month(today.year(),today.month()-1)};
 (a,b,c,d)
}
pub(crate) fn percentage(current:Money,previous:Money)->String{
 if previous==Money::ZERO {if current==Money::ZERO{"0.0%".into()}else{"New spending".into()}}
 else {format!("{:+.1}%",(current-previous)/previous*Money::from(100))}
}
pub(crate) fn amount(value:Money)->String{format!("{} {}",crate::expenses::money(value),crate::regional::current().symbol())}
#[component]
pub fn ExpenseComparison(db_form:DbForm,actor:pos_core::auth::Session)->Element{
 let mut kind=use_signal(||0usize);
 let mut dates=use_signal(||preset(chrono::Local::now().date_naive(),0));
 let mut data=use_resource(move||{let form=db_form.clone();let actor=actor.clone();async move{async{
  let pool=pos_core::connect(&form.database_config()?).await?;
  let rows=expenses::list(&pool,&actor).await?;
  let categories:Vec<String>=expenses::comparison_categories(&pool).await?;
  Ok::<_,anyhow::Error>((rows,categories))
 }.await.map_err(|e|format!("{e:#}"))}});
 let (a,b,c,d)=dates();
 let invalid=a>b||c>d;
 let state=data.read();
 let mut totals=BTreeMap::<String,(Money,Money)>::new();
 if let Some(Ok((rows,categories)))=state.as_ref(){
  for name in categories{totals.entry(name.clone()).or_default();}
  for row in rows{let entry=totals.entry(row.category.clone()).or_default();if row.expense_date>=a&&row.expense_date<=b{entry.0+=row.amount;}if row.expense_date>=c&&row.expense_date<=d{entry.1+=row.amount;}}
 }
 let current:Money=totals.values().map(|v|v.0).sum();let previous:Money=totals.values().map(|v|v.1).sum();
 let mut changes=totals.iter().map(|(n,(x,y))|(n.clone(),*x-*y,*x,*y)).collect::<Vec<_>>();
 changes.sort_by(|a,b|a.1.cmp(&b.1).then_with(||a.0.cmp(&b.0)));
 rsx!{section{class:"expense_comparison",
  button{hidden:true,"data-page-refresh":"true",onclick:move |_|data.restart()}
  header{class:"comparison_heading",h3{"Expense Comparison"}}
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
     span{class:"comparison_label",if current<previous{"↓ Spending decreased"}else if current>previous{"↑ Spending increased"}else{"→ Spending unchanged"}}
     p{class:"comparison_amount","{amount((current-previous).abs())}"}
     small{"{percentage(current,previous)} compared with the previous period"}
    }
   }
   div{class:"expense_comparison_highlights",
    for (title,increase) in [("Biggest Decreases (Savings)",false),("Biggest Increases (Spending)",true)]{
     section{class:if increase{"increased"}else{"decreased"},h3{"{title}"}p{class:"comparison_section_hint","Top 5 categories by change in amount"}
      if !changes.iter().any(|v|if increase{v.1>Money::ZERO}else{v.1<Money::ZERO}){p{"No changes in this direction."}}
      for row in if increase{changes.iter().rev().filter(|v|v.1>Money::ZERO).take(5).collect::<Vec<_>>()}else{changes.iter().filter(|v|v.1<Money::ZERO).take(5).collect::<Vec<_>>()}{
       div{class:"expense_comparison_change",span{class:"comparison_category_name","{row.0}"}span{"{amount(row.1)}"}small{"{percentage(row.2,row.3)}"}}
      }
     }
    }
   }
   section{class:"comparison_details",header{h3{"All categories"}p{"Current and previous amounts shown side by side. New spending means the previous amount was zero."}}
   div{class:"customers_table_scroll",table{class:"customers_table",thead{tr{for label in ["Category","Current Period","Previous Period","Difference","Change %","Trend"]{th{"{label}"}}}}tbody{
    for (name,(x,y)) in &totals{tr{td{"{name}"}td{"{amount(*x)}"}td{"{amount(*y)}"}td{"{amount(*x-*y)}"}td{"{percentage(*x,*y)}"}td{span{class:if x>y{"comparison_trend increased"}else if x<y{"comparison_trend decreased"}else{"comparison_trend unchanged"},if x>y{"↑ Increased"}else if x<y{"↓ Decreased"}else{"→ No change"}}}}}
   }}}
   }
  }
 }}
}
#[cfg(test)]mod tests{use super::*;#[test]fn calendar_and_zero_baseline(){assert_eq!(preset(NaiveDate::from_ymd_opt(2024,1,10).unwrap(),0),("2024-01-01".into(),"2024-01-31".into(),"2023-12-01".into(),"2023-12-31".into()));assert_eq!(month(2024,2).1,"2024-02-29");assert_eq!(percentage(10.into(),0.into()),"New spending");assert_eq!(percentage(50.into(),100.into()),"-50.0%");}}
