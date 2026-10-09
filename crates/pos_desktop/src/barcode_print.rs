use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::Product;
use serde::Serialize;

// Same Code128 symbol patterns as the Python POS barcode dialog.
const PATTERNS: [&str; 107] = ["212222","222122","222221","121223","121322","131222","122213","122312","132212","221213","221312","231212","112232","122132","122231","113222","123122","123221","223211","221132","221231","213212","223112","312131","311222","321122","321221","312212","322112","322211","212123","212321","232121","111323","131123","131321","112313","132113","132311","211313","231113","231311","112133","112331","132131","113123","113321","133121","313121","211331","231131","213113","213311","213131","311123","311321","331121","312113","312311","332111","314111","221411","431111","111224","111422","121124","121421","141122","141221","112214","112412","122114","122411","142112","142211","241211","221114","413111","241112","134111","111242","121142","121241","114212","124112","124211","411212","421112","421211","212141","214121","412121","111143","111341","131141","114113","114311","411113","411311","113141","114131","311141","411131","211412","211214","211232","2331112"];

// Retail EAN-13 fits a 30 mm / 203 DPI label at two dots per module.
// Only use it when the stored check digit is valid; never rewrite the barcode value.
fn ean13(code: &str) -> Option<Vec<u8>> {
    if code.len()!=13 || !code.bytes().all(|v|v.is_ascii_digit()) {return None;}
    let digits:Vec<usize>=code.bytes().map(|v|(v-b'0')as usize).collect();
    let sum:usize=digits[..12].iter().enumerate().map(|(i,v)|v*if i%2==0{1}else{3}).sum();
    if (10-sum%10)%10!=digits[12]{return None;}
    const L:[&str;10]=["0001101","0011001","0010011","0111101","0100011","0110001","0101111","0111011","0110111","0001011"];
    const G:[&str;10]=["0100111","0110011","0011011","0100001","0011101","0111001","0000101","0010001","0001001","0010111"];
    const R:[&str;10]=["1110010","1100110","1101100","1000010","1011100","1001110","1010000","1000100","1001000","1110100"];
    const PARITY:[&str;10]=["LLLLLL","LLGLGG","LLGGLG","LLGGGL","LGLLGG","LGGLLG","LGGGLL","LGLGLG","LGLGGL","LGGLGL"];
    let mut bits="0".repeat(11);bits.push_str("101");
    for(i,v)in digits[1..7].iter().enumerate(){bits.push_str(if PARITY[digits[0]].as_bytes()[i]==b'L'{L[*v]}else{G[*v]});}
    bits.push_str("01010");for v in &digits[7..]{bits.push_str(R[*v]);}bits.push_str("101");bits.push_str(&"0".repeat(11));
    Some(bits.bytes().map(|v|v-b'0').collect())
}

fn encode(code: &str) -> Result<Vec<u8>, String> {
    if code.is_empty() || code.len()>80 || !code.bytes().all(|b| (32..=126).contains(&b)) {
        return Err("Barcode / SKU must contain 1–80 printable English letters, digits or symbols.".into());
    }
    if let Some(bars)=ean13(code){return Ok(bars);}
    let numeric=code.len()>=4 && code.bytes().all(|v|v.is_ascii_digit());
    let mut values=vec![if numeric {105usize}else{104}];
    if numeric {
        for pair in code.as_bytes().chunks_exact(2){values.push(((pair[0]-b'0')*10+pair[1]-b'0') as usize);}
        if code.len()%2==1 {values.push(100);values.push((code.as_bytes()[code.len()-1]-32) as usize);}
    }
    else {values.extend(code.bytes().map(|v|(v-32) as usize));}
    let checksum=(values[0]+values.iter().skip(1).enumerate().map(|(i,v)|(i+1)*v).sum::<usize>())%103;
    values.extend([checksum,106]);
    let mut bars=vec![0;10];
    for value in values {for (i,width) in PATTERNS[value].bytes().enumerate(){bars.extend(std::iter::repeat_n(if i%2==0{1}else{0},(width-b'0') as usize));}}
    bars.extend([0;10]);Ok(bars)
}

#[derive(Clone, PartialEq, Serialize)]
struct Label {
    name:String, code:String, price:String, width:f64, height:f64, dpi:u32,
    show_name:bool, show_price:bool, show_number:bool,
}
impl Label {
    fn validate(&self)->Result<Vec<u8>,String> {
        if !self.width.is_finite() || !self.height.is_finite() || !(25.0..=110.0).contains(&self.width) || !(20.0..=100.0).contains(&self.height) || ![203,300].contains(&self.dpi) {return Err("Label size: width 25–110 mm, height 20–100 mm.".into());}
        let bars=encode(&self.code)?;
        let dots=(self.width*self.dpi as f64/25.4).floor() as usize;
        if dots/bars.len()<2 {return Err("Barcode is too long for this label at the selected DPI. Use a wider label.".into());}
        Ok(bars)
    }
}
async fn run(payload:serde_json::Value)->Result<String,String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)] {
            use std::{io::Write,os::windows::process::CommandExt,process::{Command,Stdio}};
            let mut child=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",concat!("[Console]::InputEncoding=[System.Text.Encoding]::UTF8;\n",include_str!("../assets/print-barcode.ps1"))])
                .creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e|e.to_string())?;
            let write=child.stdin.take().unwrap().write_all(&serde_json::to_vec(&payload).map_err(|e|e.to_string())?);
            let output=child.wait_with_output().map_err(|e|e.to_string())?;write.map_err(|e|e.to_string())?;
            if !output.status.success(){return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());}
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        #[cfg(not(windows))] {let _=payload;Err("Barcode printing requires Windows.".into())}
    }).await.map_err(|e|e.to_string())?
}
async fn preview(label:Label)->Result<(Label,String),String> {
    let bars=label.validate()?;
    let png=run(serde_json::json!({"mode":"preview","label":label,"bars":bars})).await?;
    Ok((label,png))
}

#[component]
pub fn BarcodePrint(product:Product,db_form:DbForm,on_close:EventHandler<()>) -> Element {
    let mut choice=use_signal(||0usize);
    let mut width=use_signal(||crate::local_appearance::load().unwrap_or_default().get("barcode_label_width").cloned().unwrap_or_else(||"40".into()));
    let mut height=use_signal(||crate::local_appearance::load().unwrap_or_default().get("barcode_label_height").cloned().unwrap_or_else(||"30".into()));
    let mut dpi=use_signal(||crate::local_appearance::load().unwrap_or_default().get("barcode_label_dpi").and_then(|v|v.parse::<u32>().ok()).filter(|v|[203,300].contains(v)).unwrap_or(203));let mut copies=use_signal(||"1".to_string());
    let show_name=use_signal(||true);let show_number=use_signal(||true);let show_price=use_signal(||true);
    let mut busy=use_signal(||false);let mut message=use_signal(String::new);
    let variants=product.variants.clone();
    let mut targets=Vec::new();
    if crate::is_variant_product(&product) {
        for v in variants {
            let name=format!("{} {} {}",product.name,v.size.unwrap_or_default(),v.color.unwrap_or_default());
            let code=v.barcode.filter(|s|!s.trim().is_empty()).or(v.sku).unwrap_or_default();
            targets.push((name,code,crate::format_ks(v.price)));
        }
    } else {targets.push((product.name.clone(),product.barcode.clone().filter(|s|!s.trim().is_empty()).or(product.sku.clone()).unwrap_or_default(),crate::format_ks(product.price)));}
    let names=targets.iter().map(|t|t.0.clone()).collect::<Vec<_>>();
    let label=use_memo(move || {
        let t=targets.get(choice()).cloned().unwrap_or_default();
        Label{name:t.0,code:t.1,price:t.2,width:width().parse().unwrap_or(0.0),height:height().parse().unwrap_or(0.0),dpi:dpi(),show_name:show_name(),show_number:show_number(),show_price:show_price()}
    });
    let image=use_resource(move ||preview(label()));
    let source=db_form.clone();
    let settings=use_resource(move ||{let source=source.clone();async move {
        let pool=pos_core::connect(&source.database_config().map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?;
        let values=pos_core::db::list_settings(&pool).await.map_err(|e|e.to_string())?;
        Ok::<_,String>(values.into_iter().find(|v|v.key=="barcode_printer_name").and_then(|v|v.value).unwrap_or_default())
    }});
    let printer=settings.read().as_ref().and_then(|v|v.as_ref().ok()).cloned().unwrap_or_default();
    let current=image.read().as_ref().and_then(|v|v.as_ref().ok()).cloned().filter(|(spec,_)|*spec==label());
    let quantity=copies().parse::<u32>().ok().filter(|n|(1..=500).contains(n));
    rsx! {
        div {class:"modal_backdrop",section {class:"customer_dialog barcode_print_dialog",role:"dialog",aria_modal:"true",aria_label:"Barcode Print",
            h2 {"Barcode Print"}
            div {class:"barcode_print_columns",
                div {class:"barcode_print_controls",
                    if names.len()>1 {label {"Variant" select {disabled:busy(),value:"{choice}",onchange:move |e|choice.set(e.value().parse().unwrap_or(0)),for (i,name) in names.iter().enumerate(){option {value:"{i}","{name}"}}}}}
                    label {"Barcode / SKU" input {readonly:true,value:label().code}}
                    div {class:"barcode_label_presets",
                        button {disabled:busy(),onclick:move |_|{width.set("30".into());height.set("20".into());},"30 × 20 mm"}
                        button {disabled:busy(),onclick:move |_|{width.set("40".into());height.set("30".into());},"40 × 30 mm"}
                    }
                    div {class:"barcode_print_sizes",
                        label {"Width (mm)" input {r#type:"number",min:25,max:110,disabled:busy(),value:width(),oninput:move |e|width.set(e.value())}}
                        label {"Height (mm)" input {r#type:"number",min:20,max:100,disabled:busy(),value:height(),oninput:move |e|height.set(e.value())}}
                        label {"DPI" select {disabled:busy(),value:"{dpi}",onchange:move |e|dpi.set(e.value().parse().unwrap_or(203)),option {value:"203","203"} option {value:"300","300"}}}
                        label {"Copies" input {r#type:"number",min:1,max:500,disabled:busy(),value:copies(),oninput:move |e|copies.set(e.value())}}
                    }
                    for (title,mut enabled) in [("Product name",show_name),("Barcode number",show_number),("Price",show_price)] {
                        label {class:"barcode_print_toggle",input {r#type:"checkbox",disabled:busy(),checked:enabled(),onchange:move |e|enabled.set(e.checked())} span {"{title}"}}
                    }
                    small {"Printer: {printer}"}
                    if printer.is_empty(){p {role:"alert","Select Barcode Printer in Settings > Printer."}}
                    if let Some(Err(error))=settings.read().as_ref(){p {role:"alert","{error}"}}
                }
                div {class:"barcode_label_preview",
                    if let Some((_,png))=&current {img {src:"data:image/png;base64,{png}",alt:"Barcode label preview"}}
                    else if let Some(Err(error))=image.read().as_ref(){p {role:"alert","{error}"}}
                    else {p {role:"status","Loading preview..."}}
                }
            }
            div {class:"customers_actions",
                button {disabled:busy(),onclick:move |_|on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
                button {class:"customer_primary",disabled:busy()||current.is_none()||printer.is_empty()||quantity.is_none(),onclick:move |_|{
                    if busy(){return;}let Some((spec,png))=current.clone()else{return;};let Some(copies)=quantity else{return;};let printer=printer.clone();
                    busy.set(true);spawn(async move {
                        let result=run(serde_json::json!({"mode":"print","label":spec,"png":png,"printer":printer,"copies":copies})).await;
                        if result.is_ok(){let _=crate::local_appearance::save(&[("barcode_label_width".into(),spec.width.to_string()),("barcode_label_height".into(),spec.height.to_string()),("barcode_label_dpi".into(),spec.dpi.to_string())]);}
                        busy.set(false);message.set(match result {Ok(_)=>"Barcode labels sent to printer.".into(),Err(e)=>e});
                    });
                },if busy(){"Printing..."}else{"Print"}}
            }
        }}
        if !message().is_empty(){MessageBox {title:"Barcode Print".to_string(),message:message(),on_close:move |_|message.set(String::new())}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn encoding_and_size_guards() {
        assert!(encode("").is_err());assert!(encode("မြန်မာ").is_err());assert!(encode("A\n").is_err());
        let bars=encode("123456").unwrap();assert_eq!(bars.len(),88);assert!(bars[..10].iter().all(|v|*v==0));assert!(bars[bars.len()-10..].iter().all(|v|*v==0));
        for (i,p) in PATTERNS.iter().enumerate(){assert_eq!(p.bytes().map(|v|(v-b'0')as usize).sum::<usize>(),if i==106{13}else{11});}
        // Start C (105), pairs 12/34/56, weighted checksum 44, stop.
        let expected:[usize;6]=[105,12,34,56,44,106];let mut raw=Vec::new();for v in expected {for(i,n)in PATTERNS[v].bytes().enumerate(){raw.extend(std::iter::repeat_n(if i%2==0{1}else{0},(n-b'0')as usize));}}assert_eq!(&bars[10..bars.len()-10],raw.as_slice());
        let mut label=Label{name:"Name".into(),code:"123456".into(),price:"100 Ks".into(),width:40.0,height:30.0,dpi:203,show_name:true,show_price:true,show_number:true};
        assert!(label.validate().is_ok());
        label.code="4892931360690".into();label.width=30.0;label.height=20.0;label.dpi=300;
        assert_eq!(encode(&label.code).unwrap().len(),117);
        label.dpi=203;
        assert!(label.validate().is_ok());
        assert!(ean13("4892931360691").is_none());
        let ean=ean13("4006381333931").unwrap();
        assert_eq!(&ean[11..14], &[1,0,1]);assert_eq!(&ean[56..61], &[0,1,0,1,0]);
        label.code="A".repeat(80);assert!(label.validate().is_err());label.width=f64::NAN;assert!(label.validate().is_err());
    }
}
