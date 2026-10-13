use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const KEY: &str = "customer_display_editor_v1";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    pub font_size: u16,
    pub total_size: u16,
    pub row_padding: u16,
    pub page_padding: u16,
    pub background: String,
    pub text: String,
    pub accent: String,
    pub welcome: String,
    pub thanks: String,
    pub show_received: bool,
    pub show_change: bool,
}
impl Default for Design {
    fn default() -> Self {
        Self {font_size:20,total_size:32,row_padding:8,page_padding:18,
            background:"#ffffff".into(),text:"#19252a".into(),accent:"#08665b".into(),
            welcome:"Welcome".into(),thanks:"Thank you".into(),show_received:true,show_change:true}
    }
}
impl Design {
    fn valid(&self) -> bool {
        (14..=32).contains(&self.font_size) && (24..=56).contains(&self.total_size)
            && (4..=20).contains(&self.row_padding) && (8..=40).contains(&self.page_padding)
            && [&self.background,&self.text,&self.accent].iter().all(|s| s.len()==7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit()))
            && self.welcome.chars().count()<=120 && self.thanks.chars().count()<=120
    }
    pub fn from_settings(settings: &HashMap<String,String>) -> Self {
        settings.get(KEY).and_then(|s|serde_json::from_str::<Self>(s).ok()).filter(Self::valid).unwrap_or_default()
    }
    pub fn css(&self) -> String {
        format!("body {{background:{};color:{};font-size:{}px;}} main {{padding:{}px;}} th,td {{padding-top:{}px;padding-bottom:{}px;}} header {{font-size:{}px;}} footer > div:not(.total),footer p,th {{color:inherit;opacity:.8;}} .total {{color:{};font-size:{}px;}}",
            self.background,self.text,self.font_size,self.page_padding,self.row_padding,self.row_padding,self.font_size+3,self.accent,self.total_size) + &format!(" th {{background:{};}}", self.background)
    }
}
fn escape(s:&str)->String {s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;")}
fn preview(design:&Design, welcome:bool, shop:&str, logo:Option<&str>)->String {
    let brand_logo=logo.map(|v|format!("<img src=\"{}\" alt=\"Shop logo\">",escape(v))).unwrap_or_default();
    let content=if welcome {format!("<section class='welcome'><h1>{}</h1></section>",escape(&design.welcome))} else {
        format!("<section class='sale'><div class='items'><table><thead><tr><th>Item</th><th>Qty</th><th>Amount</th></tr></thead><tbody><tr><td>Coffee</td><td>2</td><td>6,000 Ks</td></tr><tr><td>Milk bread</td><td>1</td><td>2,500 Ks</td></tr></tbody></table></div><footer><div class='total'><span>Total</span><strong>8,500 Ks</strong></div>{}{}<p>{}</p></footer></section>",
            if design.show_received {"<div><span>Received</span><strong>10,000 Ks</strong></div>"}else{""},
            if design.show_change {"<div><span>Change</span><strong>1,500 Ks</strong></div>"}else{""},escape(&design.thanks))
    };
    format!("<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><style>{}\n{}</style></head><body><main><header class='display_brand'>{brand_logo}<span>{}</span></header>{content}</main></body></html>",include_str!("../assets/customer-display.css"),design.css(),escape(shop))
}

#[component]
pub fn Editor(settings:HashMap<String,String>,on_saved:EventHandler<HashMap<String,String>>) -> Element {
    let initial=Design::from_settings(&settings);
    let mut draft=use_signal(||initial.clone());
    let mut saved=use_signal(||initial);
    let mut welcome=use_signal(||false);
    let mut notice=use_signal(String::new);
    rsx! {
        style { {include_str!("../assets/display-editor.css")} }
        div {class:"settings_organized display_editor_page",
        p {class:"cd_description","Customize the customer screen on this computer. Preview changes here, then save to apply to the connected display."}
        div {class:"cd_editor",
            div {class:"cd_controls",
                div {class:"cd_group_heading",strong {"Layout & text"} small {"Adjust spacing and readability"}}
                for (key,label,min,max,value) in [
                    ("font","Item text size",14,32,draft().font_size),
                    ("total","Total text size",24,56,draft().total_size),
                    ("row","Item row spacing",4,20,draft().row_padding),
                    ("page","Screen padding",8,40,draft().page_padding)] {
                    label {"{label} · {value}px"
                        input {r#type:"range",min:"{min}",max:"{max}",value:"{value}",oninput:move |e|{
                            if let Ok(v)=e.value().parse::<u16>() {let mut d=draft.write();match key {"font"=>d.font_size=v,"total"=>d.total_size=v,"row"=>d.row_padding=v,_=>d.page_padding=v};notice.set(String::new());}
                        }}
                    }
                }
                div {class:"cd_group_heading",strong {"Colors & messages"}}
                div {class:"cd_colors",
                    for (key,label,value) in [("background","Background",draft().background),("text","Text",draft().text),("accent","Total",draft().accent)] {
                        label {"{label}" input {r#type:"color",value:"{value}",oninput:move |e|{let mut d=draft.write();match key {"background"=>d.background=e.value(),"text"=>d.text=e.value(),_=>d.accent=e.value()};notice.set(String::new());}}}
                    }
                }
                label {"Welcome message" input {value:"{draft().welcome}",maxlength:120,oninput:move |e|{draft.write().welcome=e.value();notice.set(String::new());}}}
                label {"Thank-you message" input {value:"{draft().thanks}",maxlength:120,oninput:move |e|{draft.write().thanks=e.value();notice.set(String::new());}}}
                div {class:"cd_checks",
                    label {input {r#type:"checkbox",checked:draft().show_received,onchange:move |e|draft.write().show_received=e.checked()} "Show received amount"}
                    label {input {r#type:"checkbox",checked:draft().show_change,onchange:move |e|draft.write().show_change=e.checked()} "Show change"}
                }
            }
            div {class:"cd_preview",
                header {strong {"Preview"} label {input {r#type:"checkbox",checked:welcome(),onchange:move |e|welcome.set(e.checked())} "Welcome screen"}}
                iframe {title:"Customer display preview","sandbox":"",srcdoc:preview(&draft(),welcome(),&crate::setting_value(&settings,"shop_name","KAY POS"),crate::receipt_layout::logo(&settings).as_deref())}
                small {class:"cd_preview_hint","Sample items and amounts · Shop name and logo come from Receipt settings"}
            }
        }
        div {class:"settings_page_actions cd_actions",
            button {onclick:move |_|{draft.set(Design::default());notice.set(String::new());},"Restore defaults"}
            button {disabled:draft()==saved(),onclick:move |_|{draft.set(saved());notice.set(String::new());},"Reset changes"}
            button {class:"settings_save",disabled:draft()==saved(),onclick:move |_|{
                let design=draft();
                if !design.valid(){notice.set("Choose valid sizes, colors and messages.".into());return;}
                let value=serde_json::to_string(&design).expect("display design");
                match crate::local_appearance::save(&[(KEY.into(),value.clone())]) {
                    Ok(())=>{saved.set(design);on_saved.call(HashMap::from([(KEY.into(),value)]));notice.set("Customer display settings saved.".into());}
                    Err(e)=>notice.set(format!("Could not save: {e:#}")),
                }
            },"Save display"}
        }
        if !notice().is_empty(){p {role:"status","{notice}"}}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_uses_shop_branding_and_escapes_the_shop_name() {
        let d=Design::default();
        let html=preview(&d,false,"My <Shop> & Co",Some("data:image/png;base64,AAAA"));
        assert!(html.contains("My &lt;Shop&gt; &amp; Co"));
        assert!(html.contains("src=\"data:image/png;base64,AAAA\""));
        assert!(html.contains("class='display_brand'"));
        assert!(!preview(&d,true,"Shop",None).contains("<img"));
    }
    #[test]
    fn design_validation_and_preview_escape_user_messages() {
        let mut d=Design::default();assert!(d.valid());
        d.welcome="<script>alert(1)</script>".into();
        assert!(!preview(&d,true,"Shop",None).contains("<script>"));
        let settings=HashMap::from([(KEY.into(),serde_json::to_string(&d).unwrap())]);
        assert_eq!(Design::from_settings(&settings).welcome,d.welcome);
        d.accent="red;}body{display:none}".into();assert!(!d.valid());
        let settings=HashMap::from([(KEY.into(),serde_json::to_string(&d).unwrap())]);
        assert_eq!(Design::from_settings(&settings).accent,Design::default().accent);
        d=Design::default();d.font_size=99;assert!(!d.valid());
    }
}
