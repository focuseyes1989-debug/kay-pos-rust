use crate::regional::{Language, RegionalSettings};
use base64::Engine;
use dioxus::prelude::*;

#[component]
pub fn LanguageSwitcher() -> Element {
    let mut settings=use_context::<RegionalSettings>();
    let selected=Language::from_settings(&settings.0.read());
    let mut busy=use_signal(||false);
    let mut error=use_signal(String::new);
    let select=use_callback(move |language:Language| {
        if busy() || language==Language::from_settings(&settings.0.peek()) {return;}
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            let values=vec![("language".into(),if language==Language::English {"en"}else{"my"}.into()),("language_ui_version".into(),"1".into())];
            let result=crate::local_appearance::save(&values);
            match result {
                Ok(())=>settings.0.write().extend(values),
                Err(e)=>error.set(format!("Could not save language: {e:#}")),
            }
            busy.set(false);
        });
    });
    rsx! {
        div { class:"top_language_switch", role:"group", aria_label:"App language", aria_busy:busy(),
            for (language,title,flag) in [
                (Language::Myanmar,"Myanmar",include_str!("../assets/flag-mm.svg")),
                (Language::English,"English",include_str!("../assets/flag-gb.svg")),
            ] {
                button { r#type:"button", class:"top_language_button", title, aria_label:title,
                    aria_pressed:selected==language, disabled:busy(), onclick:move |_|select.call(language),
                    img { src:format!("data:image/svg+xml;base64,{}",base64::engine::general_purpose::STANDARD.encode(flag)), alt:"", width:24, height:16 }
                }
            }
            if !error().is_empty() {
                div { class:"top_language_error", role:"alert", "{error}"
                    button { r#type:"button", aria_label:"Dismiss language error", onclick:move |_|error.set(String::new()), "×" }
                }
            }
        }
    }
}
