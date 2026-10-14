use crate::{format_qty, icons, CartLine};
use dioxus::prelude::*;
use pos_core::models::SerialWarranty;

fn parse(text: &str, months: &str, qty: f64) -> anyhow::Result<Vec<SerialWarranty>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let months: u32 = months
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Enter warranty months from 1 to 120"))?;
    let mut seen = std::collections::HashSet::new();
    let mut units = Vec::new();
    for serial in text.lines().filter(|s| !s.trim().is_empty()) {
        let serial = pos_core::warranty::validate_serial(serial, months)?;
        anyhow::ensure!(seen.insert(serial.clone()), "Duplicate serial number");
        units.push(SerialWarranty { serial, months });
    }
    anyhow::ensure!(
        qty.fract() == 0.0 && qty == units.len() as f64,
        "Enter exactly {} serial numbers, one per sold unit",
        format_qty(qty)
    );
    Ok(units)
}

#[component]
pub(crate) fn CartSerialDialog(
    mut cart: Signal<Vec<CartLine>>,
    index: usize,
    on_close: EventHandler<()>,
) -> Element {
    let Some(line) = cart.read().get(index).cloned() else {
        return rsx! {};
    };
    let mut text = use_signal(|| {
        line.serials
            .iter()
            .map(|s| s.serial.clone())
            .collect::<Vec<_>>()
            .join("\n")
    });
    let mut months = use_signal(|| {
        line.serials
            .first()
            .map(|s| s.months)
            .unwrap_or(12)
            .to_string()
    });
    let mut error = use_signal(String::new);
    rsx! {
        div {class:"modal_backdrop",
            section {class:"cart_serial_dialog",role:"dialog",aria_modal:"true",aria_label:"Serial numbers and warranty",
                header {div {strong {"Serial & Warranty"} small {"{line.product.name} · Qty {format_qty(line.qty)}"}}
                    button {aria_label:"Close",title:"Close",onclick:move |_|on_close.call(()),icons::Icon {name:"close"}}}
                p {"Enter one serial number per line. Warranty starts on the sale date. Select a customer at checkout."}
                label {"Serial numbers" textarea {autofocus:true,rows:5,value:"{text}",placeholder:"SN-001\nSN-002",oninput:move |e|{text.set(e.value());error.set(String::new());}}}
                label {"Warranty (months, for each unit)" input {r#type:"number",min:1,max:120,value:"{months}",oninput:move |e|{months.set(e.value());error.set(String::new());}}}
                if !error().is_empty(){p {role:"alert","{error}"}}
                footer {
                    button {onclick:move |_|on_close.call(()),"Cancel"}
                    button {onclick:move |_|{text.set(String::new());error.set(String::new());},"Clear serials"}
                    button {class:"settings_save",onclick:move |_|{
                        let qty=cart.read().get(index).map(|l|l.qty).unwrap_or(0.0);
                        match parse(&text(),&months(),qty) {
                            Ok(units)=>{if let Some(line)=cart.write().get_mut(index){line.serials=units;}on_close.call(());}
                            Err(e)=>error.set(e.to_string()),
                        }
                    },"Apply"}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serial_editor_validates_unit_count_and_normalizes() {
        assert_eq!(parse(" a1 \n b2 ", "24", 2.0).unwrap()[0].serial, "A1");
        assert!(parse("a1\nA1", "12", 2.0).is_err());
        assert!(parse("a1", "12", 2.0).is_err());
        assert!(parse("a1", "0", 1.0).is_err());
        assert!(parse("a1", "12", 0.5).is_err());
        assert!(parse("", "12", 1.0).unwrap().is_empty());
    }
}
