use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Currency { Kyats, Dollar, Baht }

impl Currency {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "kyats (ks)" | "ks" | "mmk" | "kyats" => Some(Self::Kyats),
            "dollar ($)" | "$" | "usd" | "dollar" => Some(Self::Dollar),
            "baht (b)" | "b" | "thb" | "baht" => Some(Self::Baht),
            _ => None,
        }
    }
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        settings.get("currency").and_then(|v| Self::parse(v))
            .or_else(|| settings.get("currency_symbol").and_then(|v| Self::parse(v)))
            .unwrap_or(Self::Kyats)
    }
    pub fn symbol(self) -> &'static str {
        match self { Self::Kyats => "Ks", Self::Dollar => "$", Self::Baht => "B" }
    }
    pub fn name(self) -> &'static str {
        match self { Self::Kyats => "Kyats (Ks)", Self::Dollar => "Dollar ($)", Self::Baht => "Baht (B)" }
    }
    pub fn format(self, value: f64) -> String {
        let value = if value.is_finite() { value } else { 0.0 };
        let text = match self {
            Self::Kyats => format!("{:.0}", value.round()),
            _ => format!("{value:.2}"),
        };
        let (sign, text) = text.strip_prefix('-').map_or(("", text.as_str()), |s| ("-", s));
        let (whole, fraction) = text.split_once('.').map_or((text, None), |(a,b)| (a, Some(b)));
        let mut grouped = String::from(sign);
        for (index, ch) in whole.chars().enumerate() {
            if index > 0 && (whole.len()-index)%3 == 0 { grouped.push(','); }
            grouped.push(ch);
        }
        if let Some(fraction) = fraction { grouped.push('.'); grouped.push_str(fraction); }
        format!("{grouped} {}", self.symbol())
    }
}

#[derive(Clone, Copy)]
pub struct RegionalSettings(pub Signal<HashMap<String,String>>);

// Reading the shared signal subscribes each money-rendering component to changes.
// Background printing uses Currency::from_settings explicitly instead.
pub fn current() -> Currency {
    try_consume_context::<RegionalSettings>()
        .map(|settings| Currency::from_settings(&settings.0.read()))
        .unwrap_or(Currency::Kyats)
}

pub fn normalize(settings: &mut HashMap<String,String>) {
    // Previously language was a nonfunctional selector, often saved as "en".
    // Preserve the existing mixed UI as Myanmar until a new explicit selection.
    if settings.get("language_ui_version").map(String::as_str) != Some("1") {
        settings.insert("language".into(), "my".into());
    }
    let currency = Currency::from_settings(settings);
    settings.insert("currency".into(), currency.name().into());
    settings.insert("currency_symbol".into(), currency.symbol().into());
}

#[cfg(test)]
mod tests {
    use super::*;
    thread_local! {
        static RENDERED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    #[component]
    fn MoneyProbe() -> Element {
        let value = crate::format_ks(2500.25);
        RENDERED.with(|values| values.borrow_mut().push(value.clone()));
        rsx! { span { "{value}" } }
    }
    #[component]
    fn TestApp() -> Element {
        let settings = use_signal(HashMap::new);
        use_context_provider(move || RegionalSettings(settings));
        rsx! { MoneyProbe {} }
    }
    #[test]
    fn child_components_refresh_when_currency_changes() {
        RENDERED.with(|values| values.borrow_mut().clear());
        let mut app = VirtualDom::new(TestApp);
        app.rebuild_in_place();
        for (name, expected) in [("Dollar ($)", "2,500.25 $"), ("Baht (B)", "2,500.25 B"), ("Kyats (Ks)", "2,500 Ks")] {
            app.in_scope(ScopeId::APP, || {
                let mut context = consume_context::<RegionalSettings>();
                context.0.write().insert("currency".into(), name.into());
            });
            app.render_immediate_to_vec();
            RENDERED.with(|values| assert_eq!(values.borrow().last().unwrap(), expected));
        }
    }
    #[test]
    fn currency_selection_overrides_stale_symbol() {
        for (value, symbol) in [("Dollar ($)", "$"), ("THB", "B"), ("MMK", "Ks")] {
            let mut settings = HashMap::from([("currency".into(), value.into()), ("currency_symbol".into(), "Ks".into())]);
            normalize(&mut settings);
            assert_eq!(settings["currency_symbol"], symbol);
            assert_eq!(Currency::from_settings(&settings).symbol(), symbol);
        }
        assert_eq!(Currency::from_settings(&HashMap::new()), Currency::Kyats);
        assert_eq!(Currency::from_settings(&HashMap::from([("currency_symbol".into(), "$".into())])), Currency::Dollar);
    }
    #[test]
    fn formatting_preserves_amount_and_foreign_currency_cents() {
        assert_eq!(Currency::Kyats.format(2500.0), "2,500 Ks");
        assert_eq!(Currency::Dollar.format(2500.25), "2,500.25 $");
        assert_eq!(Currency::Baht.format(-1234.5), "-1,234.50 B");
        assert_eq!(Currency::Dollar.format(0.0), "0.00 $");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language { Myanmar, English }
impl Language {
    pub fn from_settings(settings: &HashMap<String,String>) -> Self {
        if settings.get("language_ui_version").map(String::as_str)==Some("1")
            && settings.get("language").map(String::as_str)==Some("en") {Self::English} else {Self::Myanmar}
    }
}
pub fn language() -> Language {
    try_consume_context::<RegionalSettings>()
        .map(|settings| Language::from_settings(&settings.0.read()))
        .unwrap_or(Language::Myanmar)
}
pub fn text(value: &str) -> String { translate(value, language()) }
fn catalog() -> &'static Vec<(String,String)> {
    static CATALOG: std::sync::OnceLock<Vec<(String,String)>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut entries:Vec<(String,String)>=serde_json::from_str(include_str!("../assets/english.json")).expect("valid English catalog");
        entries.sort_by_key(|(my,_)|std::cmp::Reverse(my.split('{').map(|part|part.split_once('}').map_or(part,|(_,tail)|tail).len()).sum::<usize>()));
        entries
    })
}
// Match entire message templates. Captured record names/values are copied verbatim.
fn template_values<'a>(template: &str, value: &'a str) -> Option<HashMap<String,&'a str>> {
    let mut pattern=template;
    let mut remaining=value;
    let mut values=HashMap::new();
    while let Some(open)=pattern.find('{') {
        remaining=remaining.strip_prefix(&pattern[..open])?;
        let tail=&pattern[open+1..];
        let close=tail.find('}')?;
        let name=&tail[..close];
        pattern=&tail[close+1..];
        let fixed=pattern.split('{').next().unwrap_or_default();
        let end=if fixed.is_empty() {if pattern.is_empty(){remaining.len()}else{return None}}else{remaining.find(fixed)?};
        let captured=&remaining[..end];
        if let Some(previous)=values.insert(name.to_owned(),captured) {if previous!=captured {return None;}}
        remaining=&remaining[end..];
    }
    if remaining==pattern {Some(values)} else {None}
}
pub fn translate(value: &str, language: Language) -> String {
    if language==Language::Myanmar {return value.into();}
    if let Some((_,english))=catalog().iter().find(|(my,_)| my==value) {return english.clone();}
    for (my,english) in catalog().iter().filter(|(my,_)|my.contains('{')) {
        if let Some(values)=template_values(my,value) {
            let mut output=String::new();
            let mut rest=english.as_str();
            while let Some(open)=rest.find('{') {
                output.push_str(&rest[..open]);
                let tail=&rest[open+1..];
                let Some(close)=tail.find('}') else{return value.into()};
                let Some(captured)=values.get(&tail[..close]) else{return value.into()};
                if matches!(&tail[..close],"title"|"percent"|"limit"|"payment"|"e:#"|"error:#"|"error") {
                    output.push_str(&translate(captured,Language::English));
                } else {output.push_str(captured);}
                rest=&tail[close+1..];
            }
            output.push_str(rest);
            return output;
        }
    }
    value.into()
}

#[cfg(test)]
mod language_tests {
    use super::*;
    #[test]
    fn legacy_selection_is_myanmar_until_explicit_selection() {
        let mut settings=HashMap::from([("language".into(),"en".into())]);
        normalize(&mut settings);
        assert_eq!(settings["language"],"my");
        settings.insert("language_ui_version".into(),"1".into());
        settings.insert("language".into(),"en".into());
        normalize(&mut settings);
        assert_eq!(Language::from_settings(&settings),Language::English);
        settings.insert("language".into(),"my".into());
        assert_eq!(Language::from_settings(&settings),Language::Myanmar);
    }
    #[test]
    fn templates_preserve_business_names_and_amounts() {
        let text="မေးထားတဲ့ကာလအတွင်း ရေခဲမုန့် ရောင်းရငွေ စုစုပေါင်း 12500 ကျပ် ရှိပါတယ်။";
        assert_eq!(translate(text,Language::English),"Total revenue for ရေခဲမုန့် in the selected period: 12500 Ks.");
        assert_eq!(translate(text,Language::Myanmar),text);
        assert_eq!(translate("ရေခဲမုန့်",Language::English),"ရေခဲမုန့်");
        assert_eq!(translate("scope=UnsoldProducts",Language::English),"scope=UnsoldProducts");
        assert_eq!(translate("ရေခဲမုန့် ပါသည့် Receipt များ",Language::English),"Receipts containing ရေခဲမုန့်");
        assert_eq!(translate("ကိုက်ညီတဲ့ receipt 5 ခု (ပထမ 50 ခု) ပြထားပါတယ်။ Receipt နံပါတ်ကို နှိပ်ပြီး အသေးစိတ် ကြည့်နိုင်ပါတယ်။",Language::English),"Showing 5 matching receipts (first 50). Click a receipt number for details.");
        let mut keys=std::collections::HashSet::new();
        for (my,en) in catalog() {assert!(keys.insert(my));assert!(!en.is_empty());}
    }
    #[allow(unused_braces)]
    #[component]
    fn LanguageProbe() -> Element { rsx! { span { {text("အသေးစိတ်")} } } }
    #[component]
    fn LanguageApp() -> Element {
        let settings=use_signal(||HashMap::from([("language".into(),"my".into()),("language_ui_version".into(),"1".into())]));
        use_context_provider(move ||RegionalSettings(settings));
        rsx! { LanguageProbe {} }
    }
    #[test]
    fn language_signal_rerenders_children() {
        let mut app=VirtualDom::new(LanguageApp);
        app.rebuild_in_place();
        app.in_scope(ScopeId::APP, || {
            let mut settings=consume_context::<RegionalSettings>();
            assert_eq!(text("အသေးစိတ်"),"အသေးစိတ်");
            settings.0.write().insert("language".into(),"en".into());
            assert_eq!(text("အသေးစိတ်"),"Details");
        });
        assert!(format!("{:?}",app.render_immediate_to_vec().edits).contains("Details"));
    }
}
