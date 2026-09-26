use crate::{icons::ActionLabel, CartLine, DbForm};
use anyhow::{ensure, Context, Result};
use dioxus::prelude::*;
use pos_core::{
    auth::Session,
    held_sales::{self as core, Action, Command, Hold},
};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf};

#[derive(Clone, Serialize, Deserialize, Default)]
struct Journal {
    owner: String,
    pending: Option<Command>,
    active: Option<String>,
}
fn path(db: &DbForm) -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is unavailable")?;
    let id = format!(
        "{}:{}:{}",
        db.host.trim().to_lowercase(),
        db.port.trim(),
        db.database.trim()
    );
    Ok(PathBuf::from(root)
        .join("KAY POS Rust/held-carts")
        .join(format!(
            "{}.json",
            pos_core::auth::fingerprint(id.as_bytes())
        )))
}
fn load(db: &DbForm) -> Result<Journal> {
    let p = path(db)?;
    if !p.exists() {
        return Ok(Journal::default());
    }
    Ok(serde_json::from_slice(&std::fs::read(p)?)?)
}
fn save(db: &DbForm, j: &Journal) -> Result<()> {
    save_path(&path(db)?, j)
}
fn save_path(p: &std::path::Path, j: &Journal) -> Result<()> {
    std::fs::create_dir_all(p.parent().unwrap())?;
    let mut file = tempfile::NamedTempFile::new_in(p.parent().unwrap())?;
    file.write_all(&serde_json::to_vec(j)?)?;
    file.as_file().sync_all()?;
    file.persist(p)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn product() -> pos_core::Product {
        serde_json::from_value(serde_json::json!({"id":1,"name":"Product","price":120.0,"cost":50.0,"stock":5.0,"low_stock":1.0,"sold_by":"Each","variants":[],"price_tiers":[]})).unwrap()
    }
    fn hold(rows: serde_json::Value) -> Hold {
        Hold {
            id: 1,
            hold_no: "H1".into(),
            cart_json: rows.to_string(),
            customer_id: None,
            customer_name: String::new(),
            payment_type: "Cash".into(),
            note: String::new(),
            total_amount: 100.0,
            item_count: 1,
            created_at: String::new(),
        }
    }
    #[test]
    fn restore_uses_current_price_and_aggregate_stock() {
        let h = hold(serde_json::json!([{"id":1,"qty":3,"price":100}]));
        assert_eq!(restore(&h, &[product()]).unwrap()[0].unit_price(), 120.0);
        let duplicate =
            hold(serde_json::json!([{"id":1,"qty":3,"price":100},{"id":1,"qty":3,"price":100}]));
        assert!(restore(&duplicate, &[product()]).is_err());
        assert!(restore(&h, &[]).is_err());
    }
    #[test]
    fn restore_rejects_missing_variants_and_main_batch_selection() {
        let h = hold(serde_json::json!([{"id":1,"qty":1,"price":100,"variant_id":99}]));
        assert!(restore(&h, &[product()]).is_err());
        let h = hold(serde_json::json!([{"id":1,"qty":1,"price":100,"location_id":2}]));
        assert!(restore(&h, &[product()]).is_err());
    }
    #[test]
    fn restore_preserves_service_quote_without_stock() {
        let mut p = product();
        p.sold_by = Some("Service".into());
        p.stock = 0.0;
        let h = hold(serde_json::json!([{"id":1,"qty":1,"price":275}]));
        assert_eq!(restore(&h, &[p]).unwrap()[0].unit_price(), 275.0);
    }
    #[test]
    fn journal_atomic_replace_keeps_original_command() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("held.json");
        let command = Command {
            request_id: pos_core::auth::new_request_id(),
            action: Action::Resume { id: 5 },
        };
        save_path(
            &p,
            &Journal {
                owner: "operator".into(),
                pending: Some(command.clone()),
                active: None,
            },
        )
        .unwrap();
        let j: Journal = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(j.pending, Some(command));
        save_path(&p, &Journal::default()).unwrap();
        let j: Journal = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
        assert!(j.pending.is_none());
    }
}
pub fn completed(db: &DbForm) -> Result<()> {
    save(db, &Journal::default())
}
fn make_hold(
    lines: &[CartLine],
    customer_id: Option<i32>,
    payment: String,
    note: String,
) -> Result<Hold> {
    let rows=lines.iter().map(|l|serde_json::json!({"id":l.product.id,"name":l.product.name,"qty":l.qty,"price":l.unit_price(),"cost":l.variant.as_ref().map(|v|v.cost).unwrap_or(l.product.cost),"is_service":crate::is_service_product(&l.product),"variant_id":l.variant.as_ref().map(|v|v.variant_id),"sold_by":l.product.sold_by})).collect::<Vec<_>>();
    let hold = Hold {
        id: 0,
        hold_no: String::new(),
        cart_json: serde_json::to_string(&rows)?,
        customer_id,
        customer_name: String::new(),
        payment_type: payment,
        note,
        total_amount: lines.iter().map(crate::line_total).sum(),
        item_count: lines.iter().map(|l| l.qty).sum::<f64>().ceil() as i32,
        created_at: String::new(),
    };
    core::validate(&hold)?;
    Ok(hold)
}
fn restore(hold: &Hold, products: &[pos_core::Product]) -> Result<Vec<CartLine>> {
    core::validate(hold)?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&hold.cart_json)?;
    let mut lines = Vec::new();
    for r in rows {
        ensure!(
            r.get("location_id").is_none_or(|v| v.is_null())
                && !r
                    .get("expiry_discount_enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            "This hold uses Main POS batch/clearance selection; resume it in Main POS"
        );
        let id = r["id"].as_i64().context("Invalid product ID")? as i32;
        let p = products
            .iter()
            .find(|p| p.id == id)
            .context("A held product was removed; return this hold for review")?
            .clone();
        ensure!(
            !p.sold_by
                .as_deref()
                .unwrap_or("")
                .eq_ignore_ascii_case("Restaurant"),
            "Restaurant holds must be resumed in Main POS"
        );
        let variant = if let Some(id) = r.get("variant_id").and_then(|v| v.as_i64()) {
            Some(
                p.variants
                    .iter()
                    .find(|v| v.variant_id as i64 == id)
                    .context("A held variant is inactive or removed")?
                    .clone(),
            )
        } else {
            None
        };
        ensure!(
            crate::is_variant_product(&p) == variant.is_some(),
            "Held product variant is missing or changed"
        );
        let qty = r["qty"].as_f64().context("Invalid quantity")?;
        let unit_price_override =
            crate::is_service_product(&p).then(|| r["price"].as_f64().unwrap_or(0.0));
        lines.push(CartLine {
            product: p,
            variant,
            qty,
            unit_price_override,
        });
    }
    let mut totals = std::collections::HashMap::new();
    for l in &lines {
        *totals
            .entry((l.product.id, l.variant.as_ref().map(|v| v.variant_id)))
            .or_insert(0.0) += l.qty;
    }
    for l in &lines {
        if !crate::is_service_product(&l.product) {
            let stock = l
                .variant
                .as_ref()
                .map(|v| v.stock)
                .unwrap_or(l.product.stock);
            ensure!(
                totals[&(l.product.id, l.variant.as_ref().map(|v| v.variant_id))] <= stock,
                "Insufficient current stock for {}",
                l.product.name
            );
        }
    }
    Ok(lines)
}

#[component]
pub fn Controls(
    db_form: DbForm,
    actor: Session,
    mut cart: Signal<Vec<CartLine>>,
    mut customer: Signal<Option<i32>>,
    mut payment: Signal<String>,
    mut active: Signal<Option<String>>,
    blocked: bool,
    visible: bool,
) -> Element {
    let mut login = use_context::<Signal<Option<Session>>>();
    let mut journal = use_signal(|| {
        load(&db_form).map_err(|e| format!("Held cart recovery file could not be read: {e:#}"))
    });
    let mut initialized = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut modal = use_signal(|| false);
    let mut holding = use_signal(|| false);
    let mut note = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut refresh = use_signal(|| 0u64);
    let source = db_form.clone();
    let who = actor.clone();
    let data = use_resource(move || {
        let db = source.clone();
        let actor = who.clone();
        let _ = refresh();
        let open = modal();
        async move {
            if !open {
                return Ok(vec![]);
            }
            async { core::list(&pos_core::connect(&db.database_config()?).await?, &actor).await }
                .await
                .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    let recovery = journal()
        .as_ref()
        .is_ok_and(|j| j.pending.is_some() || j.active.is_some())
        && !initialized();
    let db_restore = db_form.clone();
    let actor_restore = actor.clone();
    let run = move |c: Command| {
        if busy() || blocked {
            return;
        }
        let Ok(mut j) = journal() else { return };
        if !j.owner.is_empty() && j.owner != actor.username() {
            message.set(format!("Sign in as {} to recover this held cart", j.owner));
            return;
        }
        if let Some(old) = &j.pending {
            if old != &c {
                message.set("Resolve the previous hold request first".into());
                return;
            }
        }
        j.owner = actor.username().into();
        j.pending = Some(c.clone());
        if let Err(e) = save(&db_form, &j) {
            message.set(format!("{e:#}"));
            return;
        }
        journal.set(Ok(j.clone()));
        busy.set(true);
        let db = db_form.clone();
        let actor = actor.clone();
        spawn(async move {
            let result = async {
                let pool = pos_core::connect(&db.database_config()?).await?;
                let out = core::execute(&pool, &actor, &c).await?;
                if let Some(token) = &out.token {
                    // A completed/re-held session must never be loaded from an old retry result.
                    if let Some(current) = core::session(&pool, &actor, token).await? {
                        j.active = Some(token.clone());
                        j.pending = None;
                        save(&db, &j)?;
                        journal.set(Ok(j.clone()));
                        active.set(j.active.clone());
                        let mut products =
                            pos_core::db::search_product_metadata(&pool, "", 0).await?;
                        pos_core::discounts::attach(&pool, &mut products).await?;
                        let lines = restore(&current.hold, &products)?;
                        cart.set(lines);
                        customer.set(current.hold.customer_id);
                        payment.set(current.hold.payment_type);
                        message.set("Resumed with current prices and stock.".into());
                    } else {
                        j = Journal::default();
                        save(&db, &j)?;
                        active.set(None);
                    }
                } else {
                    j = Journal::default();
                    save(&db, &j)?;
                    active.set(None);
                    cart.set(vec![]);
                    customer.set(None);
                    message.set(format!("Saved {}", out.hold.hold_no));
                }
                journal.set(Ok(j));
                initialized.set(true);
                modal.set(false);
                holding.set(false);
                refresh += 1;
                Ok::<_, anyhow::Error>(())
            }
            .await;
            busy.set(false);
            if let Err(e) = result {
                message.set(format!("{e:#}"));
                initialized.set(false);
            }
        });
    };
    // Recovery is explicit: never replace a live cart automatically.
    rsx! {
        if visible {div{class:"held_toolbar",
            button{class:"held_icon_button",title:"Hold sale",aria_label:"Hold sale",disabled:blocked||busy()||cart().is_empty()||recovery,onclick:move |_|holding.set(true),ActionLabel{label:"Hold sale"}}
            button{class:"held_icon_button",title:"Held sales",aria_label:"Held sales",disabled:blocked||busy()||!cart().is_empty()||active().is_some()||recovery,onclick:move |_|{modal.set(true);refresh+=1;},ActionLabel{label:"Held sales"}}
            if active().is_some(){span{"Resumed hold"}}
            if !message().is_empty(){span{role:"status","{message}"}}
        }}
        if holding(){div{class:"modal_backdrop",section{class:"customer_dialog held_dialog",h2{"Hold sale"}
            label{"Note / customer reference" input{value:"{note}",oninput:move|e|note.set(e.value()),disabled:busy()}}
            div{class:"customers_actions",button{disabled:busy(),onclick:move |_|holding.set(false),ActionLabel{label:"Cancel"}}
                button{class:"customer_primary",disabled:busy(),onclick:{let mut run=run.clone();move |_|{match make_hold(&cart(),customer(),payment(),note()){
                    Ok(hold)=>run(Command{request_id:pos_core::auth::new_request_id(),action:Action::Save{hold,return_token:active()}}),Err(e)=>message.set(format!("{e:#}"))
                }}},ActionLabel{label:"Hold sale"}}
            }p{role:"status","{message}"}
        }}}
        if modal(){div{class:"modal_backdrop",section{class:"customer_dialog held_dialog",role:"dialog",aria_modal:"true",aria_label:"Held sales",
            header{class:"customers_header",h2{"Held sales"}button{disabled:busy(),onclick:move |_|modal.set(false),ActionLabel{label:"Close"}}}
            match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}},Some(Ok(rows))=>rsx!{
                for h in rows{article{class:"held_row",key:"{h.id}",div{strong{"{h.hold_no}"}small{"{h.customer_name} / {h.item_count} items / {crate::format_ks(h.total_amount)}"}small{"{h.note}"}small{"{h.created_at}"}}
                    button{disabled:busy()||blocked||!cart().is_empty(),onclick:{let id=h.id;let mut run=run.clone();move |_|run(Command{request_id:pos_core::auth::new_request_id(),action:Action::Resume{id}})},ActionLabel{label:"Resume sale"}}
                }}if rows.is_empty(){p{"No held sales."}}
            }}p{role:"alert","{message}"}
        }}}
        if !blocked && (journal().is_err()||recovery) {div{class:"modal_backdrop",section{class:"customer_dialog held_dialog",role:"dialog",aria_modal:"true",aria_label:"Held cart recovery",
            h2{"Held cart recovery"}p{role:"alert","{message}"}
            match journal(){Err(e)=>rsx!{p{"{e}"}},Ok(j)=>rsx!{
                p{"Operator: {j.owner}"}
                if let Some(c)=j.pending{
                    button{disabled:busy()||blocked,onclick:{let c=c.clone();let mut run=run.clone();move |_|run(c.clone())},"Retry original request"}
                    button{disabled:busy()||blocked,onclick:{let db=db_restore.clone();let actor=actor_restore.clone();move |_|{
                        if journal().as_ref().is_ok_and(|j|j.owner!=actor.username()){message.set("Sign in as the original operator first.".into());return}
                        let db=db.clone();let actor=actor.clone();let c=c.clone();busy.set(true);spawn(async move{
                            let result=async{
                                let pool=pos_core::connect(&db.database_config()?).await?;
                                ensure!(!core::cancel_pending(&pool,&actor,&c).await?,"Request was saved. Retry original request to recover it.");
                                let mut next=Journal::default();
                                if let Action::Save{hold,return_token}=&c.action{
                                    if cart().is_empty(){
                                        let mut products=pos_core::db::search_product_metadata(&pool,"",0).await?;pos_core::discounts::attach(&pool,&mut products).await?;
                                        cart.set(restore(hold,&products)?);customer.set(hold.customer_id);payment.set(hold.payment_type.clone());
                                    }
                                    next.owner=actor.username().into();next.active=return_token.clone();
                                }
                                save(&db,&next)?;active.set(next.active.clone());journal.set(Ok(next));initialized.set(true);modal.set(false);holding.set(false);message.set("Unsaved request cancelled.".into());Ok::<_,anyhow::Error>(())
                            }.await;busy.set(false);if let Err(e)=result{message.set(format!("{e:#}"));}
                        });
                    }},"Check and cancel unsaved request"}
                }
                else if let Some(token)=j.active{
                    button{disabled:busy()||blocked,onclick:{let db=db_restore.clone();let actor=actor_restore.clone();let token=token.clone();move |_|{
                        let db=db.clone();let actor=actor.clone();let token=token.clone();busy.set(true);spawn(async move{let result=async{
                            let pool=pos_core::connect(&db.database_config()?).await?;
                            if let Some(out)=core::session(&pool,&actor,&token).await?{
                                let mut products=pos_core::db::search_product_metadata(&pool,"",0).await?;pos_core::discounts::attach(&pool,&mut products).await?;
                                cart.set(restore(&out.hold,&products)?);customer.set(out.hold.customer_id);payment.set(out.hold.payment_type);active.set(Some(token));
                            }else{completed(&db)?;journal.set(Ok(Journal::default()));active.set(None);}
                            initialized.set(true);Ok::<_,anyhow::Error>(())
                        }.await;busy.set(false);if let Err(e)=result{message.set(format!("{e:#}"));}});
                    }},"Recover saved cart"}
                    button{disabled:busy()||blocked,onclick:{let db=db_restore.clone();let actor=actor_restore.clone();let run=run.clone();move |_|{let db=db.clone();let actor=actor.clone();let token=token.clone();let mut run=run.clone();busy.set(true);spawn(async move{
                        let result=async{let pool=pos_core::connect(&db.database_config()?).await?;core::session(&pool,&actor,&token).await?.context("Hold already resolved")}.await;
                        busy.set(false);
                        match result{Ok(out)=>run(Command{request_id:pos_core::auth::new_request_id(),action:Action::Save{hold:out.hold,return_token:Some(token)}}),Err(e)=>message.set(format!("{e:#}"))}
                    });}},"Return saved hold"}
                }
            }}
            button{disabled:busy(),onclick:move |_|login.set(None),ActionLabel{label:"Sign out"}}
        }}}
    }
}
