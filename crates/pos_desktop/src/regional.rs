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
