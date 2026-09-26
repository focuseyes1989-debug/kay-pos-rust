use super::*;
use anyhow::{Context, Result};
use std::io::Write;

#[derive(Clone, Serialize, Deserialize)]
pub struct PendingCheckout {
    pub draft: SaleDraft,
    pub lines: Vec<CartLine>,
}

fn path(form: &DbForm) -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .context("LOCALAPPDATA is unavailable; cannot safely save checkout")?;
    let directory = PathBuf::from(root).join("KAY POS Rust").join("pending");
    fs::create_dir_all(&directory)?;
    let identity = format!(
        "{}:{}:{}",
        form.host.trim().to_lowercase(),
        form.port.trim(),
        form.database.trim()
    );
    Ok(directory.join(format!(
        "{}.json",
        pos_core::auth::fingerprint(identity.as_bytes())
    )))
}

pub fn load(form: &DbForm) -> Result<Option<PendingCheckout>> {
    load_path(&path(form)?)
}

fn load_path(path: &std::path::Path) -> Result<Option<PendingCheckout>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).context("Pending checkout file is damaged. Do not create another sale; reconcile the receipt first")?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn save(form: &DbForm, value: &PendingCheckout) -> Result<()> {
    save_path(&path(form)?, value)
}

fn save_path(path: &std::path::Path, value: &PendingCheckout) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    // create_new prevents another process from replacing an unresolved request.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context(
            "An unresolved checkout exists. Restart to recover it before making another sale",
        )?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn clear(form: &DbForm, invoice: &str) -> Result<()> {
    anyhow::ensure!(
        load(form)?.is_some_and(|p| p.draft.invoice_no == invoice),
        "Pending checkout changed; do not overwrite it"
    );
    fs::remove_file(path(form)?).map_err(Into::into)
}

pub fn acquire_instance(form: &DbForm) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path(form)?.with_extension("lock"))
        .context("Another KAY POS client is using this database on this Windows account. Close it before continuing")
}

pub fn prepare(
    lines: Vec<CartLine>,
    payment: f64,
    discount: f64,
    payment_type: String,
    customer_id: Option<i32>,
    actor: &pos_core::auth::Session,
    settings: &HashMap<String, String>,
) -> PendingCheckout {
    let subtotal = lines.iter().map(line_total).sum::<f64>();
    let draft = SaleDraft {
        held_token:None,
        invoice_no: pos_core::auth::new_request_id(),
        customer_id,
        payment_type,
        payment,
        discount_amount: discount,
        created_by: actor.username().into(),
        expected_total: subtotal - discount + checkout_tax(settings, subtotal - discount),
        items: lines
            .iter()
            .map(|line| SaleItemDraft {
                promotion: line.product.promotion.clone(),
                product_id: line.product.id,
                variant_id: line.variant.as_ref().map(|v| v.variant_id),
                product_name: line.display_name(),
                qty: line.qty,
                price: line.unit_price(),
                cost: line
                    .variant
                    .as_ref()
                    .map(|v| v.cost)
                    .unwrap_or(line.product.cost),
                is_service: is_service_product(&line.product),
                wholesale_regular_price: line.wholesale_regular_price(),
                wholesale_savings: line.wholesale_savings(),
                wholesale_tier_min_qty: line.wholesale_tier_min_qty(),
                wholesale_unit_label: line.wholesale_unit_label(),
            })
            .collect(),
    };
    PendingCheckout { draft, lines }
}

pub async fn submit(
    form: DbForm,
    pending: PendingCheckout,
    actor: pos_core::auth::Session,
) -> Result<ReceiptData> {
    let pool = connect(&form.database_config()?).await?;
    let sale = complete_sale(&pool, &pending.draft, &actor).await?;
    let subtotal = pending.draft.items.iter().map(|i| i.qty * i.price).sum();
    Ok(ReceiptData {
        invoice_no: sale.invoice_no,
        lines: pending.lines,
        subtotal,
        discount: pending.draft.discount_amount,
        total: sale.total,
        payment_type: pending.draft.payment_type,
        payment: sale.payment,
        change: sale.change_amount,
    })
}

#[component]
pub fn Recovery(
    pending: Signal<std::result::Result<Option<PendingCheckout>, String>>,
    form: DbForm,
    saving: Signal<bool>,
    on_resolved: EventHandler<Option<ReceiptData>>,
) -> Element {
    let mut session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut error = use_signal(String::new);
    let current = pending.read().clone();
    let run = move |cancel: bool| {
        if saving() {
            return;
        }
        let Ok(Some(request)) = pending.read().clone() else {
            return;
        };
        let Some(actor) = session.read().clone() else {
            return;
        };
        let form = form.clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            let invoice = request.draft.invoice_no.clone();
            let result: Result<Option<ReceiptData>> = async {
                let cancelled = if cancel {
                    let pool = connect(&form.database_config()?).await?;
                    pos_core::sales::cancel_uncommitted(&pool, &request.draft, &actor).await?
                } else {
                    false
                };
                let receipt = if cancelled {
                    None
                } else {
                    Some(submit(form.clone(), request, actor).await?)
                };
                clear(&form, &invoice)?;
                Ok(receipt)
            }
            .await;
            saving.set(false);
            match result {
                Ok(receipt) => {
                    pending.set(Ok(None));
                    on_resolved.call(receipt);
                }
                Err(e) => error.set(format!("{e:#}")),
            }
        });
    };
    let mut retry = run.clone();
    let mut cancel = run;
    rsx! {
        div { class: "modal_backdrop checkout_recovery",
            section { class: "customer_dialog", role: "dialog", aria_modal: "true", aria_label: "Unresolved checkout",
                h2 { "Unresolved checkout" }
                match current {
                    Ok(Some(request)) => rsx! {
                        p { "Invoice: {request.draft.invoice_no}" }
                        p { "Operator: {request.draft.created_by}" }
                        p { "Resolve this sale before starting another. A saved sale will be recovered without selling it again." }
                        button { class: "primary", disabled: saving(), onclick: move |_| retry(false), "Retry original sale" }
                        button { disabled: saving(), onclick: move |_| cancel(true), "Check and cancel unsaved sale" }
                    },
                    Err(message) => rsx! { p { role: "alert", "{message}" } },
                    _ => rsx! {},
                }
                if saving() { p { role: "status", "Checking server..." } }
                if !error().is_empty() { p { role: "alert", "{error}" } }
                button { disabled: saving(), onclick: move |_| session.set(None), crate::icons::ActionLabel { label:"Sign out" } }
            }
        }
    }
}

#[test]
fn durable_request_cannot_be_overwritten_and_reloads_identically() -> Result<()> {
    let path = std::env::temp_dir().join(format!("{}.json", pos_core::auth::new_request_id()));
    let value = PendingCheckout {
        lines: vec![],
        draft: SaleDraft {
            held_token:None,
            invoice_no: pos_core::auth::new_request_id(),
            customer_id: None,
            payment_type: "Cash".into(),
            payment: 100.0,
            discount_amount: 0.0,
            expected_total: 100.0,
            created_by: "test".into(),
            items: vec![],
        },
    };
    assert!(load_path(&path)?.is_none());
    save_path(&path, &value)?;
    assert!(save_path(&path, &value).is_err());
    let restored = load_path(&path)?.unwrap();
    assert_eq!(
        serde_json::to_vec(&value.draft)?,
        serde_json::to_vec(&restored.draft)?
    );
    fs::write(&path, b"incomplete")?;
    assert!(load_path(&path).is_err());
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn only_one_instance_can_own_recovery() -> Result<()> {
    let form = DbForm {
        database: pos_core::auth::new_request_id(),
        ..Default::default()
    };
    let first = acquire_instance(&form)?;
    assert!(acquire_instance(&form).is_err());
    drop(first);
    let second = acquire_instance(&form)?;
    drop(second);
    fs::remove_file(path(&form)?.with_extension("lock"))?;
    Ok(())
}
