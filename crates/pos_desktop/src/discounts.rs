use crate::{DbForm,icons::ActionLabel,expense_management::{command,pending,Operation}};
use dioxus::prelude::*;
use pos_core::{auth::Session,discounts::{self,Discount},expense_management::{Action,Command},Product};
#[component]
pub fn DiscountsPage(db_form:DbForm,actor:Session)->Element {
    let mut refresh=use_signal(||0u64);let source=db_form.clone();let who=actor.clone();
    let data=use_resource(move||{let db=source.clone();let actor=who.clone();let _=refresh();async move{async{
        let pool=pos_core::connect(&db.database_config()?).await?;
        let campaigns=discounts::list(&pool,&actor).await?;
        let products=pos_core::db::search_product_metadata(&pool,"",0).await?;
        Ok::<_,anyhow::Error>((campaigns,products))
    }.await.map_err(|e|format!("{e:#}"))}});
    let mut search=use_signal(String::new);let mut status=use_signal(||"All".to_string());let mut page=use_signal(||0usize);
    let mut editor=use_signal(||None::<(Option<Discount>,Discount)>);let mut op=use_signal(||pending(&db_form).ok().flatten().map(|(_,c)|c));
    let error=use_hook(||pending(&db_form).err().map(|e|e.to_string()).unwrap_or_default());
    let today=chrono::Local::now().format("%Y-%m-%d").to_string();
    rsx!{section{class:"customers_page phase4_page",
        button{hidden:true,"data-page-refresh":"true",onclick:move |_|refresh+=1}
        header{class:"customers_header",h2{ActionLabel{label:"Discounts"}}
            button{class:"customer_primary",disabled:!data.finished(),onclick:move |_|{let today=chrono::Local::now().format("%Y-%m-%d").to_string();editor.set(Some((None,Discount{id:0,product_id:0,discount_percent:10.into(),discount_type:"percentage".into(),manual_price:0.into(),start_date:today.clone(),end_date:today,active:1,note:String::new()})));},ActionLabel{label:"Add promotion"}}
        }
        div{class:"phase4_toolbar",label{"Search" input{r#type:"search",placeholder:"Product, SKU or note",value:"{search}",oninput:move|e|{search.set(e.value());page.set(0);}}}
            label{"Status" select{value:"{status}",onchange:move|e|{status.set(e.value());page.set(0);},for s in ["All","Active now","Scheduled","Expired","Disabled"]{option{value:s,"{s}"}}}}
        }
        if !error.is_empty(){p{role:"alert","{error}"}}
        match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}},Some(Ok((all,products)))=>{
            let q=search().to_lowercase();let rows=all.iter().filter(|d|{let name=products.iter().find(|p|p.id==d.product_id).map(|p|format!("{} {}",p.name,p.sku.as_deref().unwrap_or(""))).unwrap_or_default();(status()=="All"||status()==d.status(&today))&&format!("{name} {}",d.note).to_lowercase().contains(&q)}).collect::<Vec<_>>();
            let pages=rows.len().div_ceil(25).max(1);let current=page().min(pages-1);
            rsx!{div{class:"phase4_table",table{thead{tr{th{"Product"}th{"Promotion"}th{"Start"}th{"End"}th{"Status"}th{"Note"}th{}}}tbody{
                for d in rows.iter().skip(current*25).take(25){tr{key:"{d.id}",td{{products.iter().find(|p|p.id==d.product_id).map(|p|p.name.as_str()).unwrap_or("Unavailable product")}}
                    td{if d.discount_type=="manual_price"{"{d.manual_price} {crate::regional::current().symbol()}"}else{"{d.discount_percent}%"}}
                    td{"{d.start_date}"}td{"{d.end_date}"}td{"{d.status(&today)}"}td{"{d.note}"}td{button{onclick:{let d=(*d).clone();move |_|editor.set(Some((Some(d.clone()),d.clone())))},ActionLabel{label:"Edit"}}}
                }}
            }}}
            if rows.is_empty(){p{"No promotions found."}}
            footer{class:"customers_pagination",button{disabled:current==0,onclick:move |_|page.set(current.saturating_sub(1)),"Previous"}span{"Page {current+1} of {pages}"}button{disabled:current+1>=pages,onclick:move |_|page.set(current+1),"Next"}}
            if let Some((old,form))=editor(){Editor{old,form,products:products.clone(),onsave:move|c|{editor.set(None);op.set(Some(c));},onclose:move |_|editor.set(None)}}
            }
        }}
    }
    if let Some(c)=op(){Operation{db_form,actor,command:c,on_done:move |_|{op.set(None);refresh+=1;}}}
    }
}
#[component]
fn Editor(old:Option<Discount>,form:Discount,products:Vec<Product>,onsave:EventHandler<Command>,onclose:EventHandler<()>)->Element{
    let mut form=use_signal(||form);let mut value=use_signal(||if form().discount_type=="manual_price"{form().manual_price.to_string()}else{form().discount_percent.to_string()});let mut error=use_signal(String::new);
    rsx!{div{class:"modal_backdrop",section{class:"customer_dialog phase4_dialog",role:"dialog",aria_modal:"true",aria_label:"Promotion",
        h2{if old.is_some(){"Edit promotion"}else{"Add promotion"}}
        div{class:"customer_form",
            label{class:"customer_remarks","Product" select{disabled:old.is_some(),value:"{form().product_id}",onchange:move|e|form.write().product_id=e.value().parse().unwrap_or(0),option{value:"0","Select product"}
                for p in products.iter().filter(|p|!matches!(p.sold_by.as_deref(),Some("Service"|"Restaurant"))){option{value:"{p.id}","{p.name} / " {p.sku.as_deref().unwrap_or("")}}}
            }}
            label{"Type" select{value:"{form().discount_type}",onchange:move|e|{form.write().discount_type=e.value();value.set(String::new());},option{value:"percentage","Percentage"}option{value:"manual_price","Promotion price"}}}
            label{if form().discount_type=="percentage"{"Discount (%)"}else{"Promotion price"}input{r#type:"number",min:"0.01",step:"0.01",value:"{value}",oninput:move|e|value.set(e.value())}}
            label{"Start" input{r#type:"date",value:"{form().start_date}",oninput:move|e|form.write().start_date=e.value()}}
            label{"End" input{r#type:"date",value:"{form().end_date}",oninput:move|e|form.write().end_date=e.value()}}
            label{input{r#type:"checkbox",checked:form().active==1,onchange:move|e|form.write().active=i32::from(e.checked())}"Enabled"}
            label{class:"customer_remarks","Note" textarea{value:"{form().note}",oninput:move|e|form.write().note=e.value()}}
        }
        p{role:"alert","{error}"}
        div{class:"customers_actions",button{onclick:move |_|onclose.call(()),ActionLabel{label:"Cancel"}}button{class:"customer_primary",onclick:move |_|{
            let mut d=form();let Ok(v)=value().parse()else{error.set("Enter a valid value".into());return};
            if d.discount_type=="percentage"{d.discount_percent=v;d.manual_price=0.into()}else{d.manual_price=v;d.discount_percent=0.into()}
            if let Err(e)=d.validate(){error.set(e.to_string());return}onsave.call(command(Action::Discount{old:old.clone(),new:d}));
        },ActionLabel{label:"Save promotion"}}}
    }}}
}
