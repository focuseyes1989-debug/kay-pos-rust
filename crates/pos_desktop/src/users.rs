use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{
    connect,
    users::{self, User},
};
#[component]
pub fn UsersSettings(db_form: DbForm) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async { users::list(&connect(&source.database_config()?).await?).await }
                .await
                .map_err(|e| format!("{e:#}"))
        }
    });
    let mut editor = use_signal(|| None::<User>);
    let mut deleting = use_signal(|| false);
    rsx! {section {class:"users_settings",
        div {class:"users_settings_toolbar",button {class:"users_add",onclick:move |_|{deleting.set(false);editor.set(Some(User{role:"Cashier".into(),active:1,..Default::default()}));},"+ Add user"} button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}}
        match data.read().as_ref() {
            None=>rsx!{p {role:"status","Loading users..."}},
            Some(Err(e))=>rsx!{p {role:"alert","{e}"}},
            Some(Ok(rows))=>rsx!{
                if rows.is_empty(){p {"No users found."}}
                for user in rows {article {class:"users_settings_row",key:"{user.id}",
                    div {class:"users_avatar",if let Some(url)=&user.avatar {img {src:"{url}",alt:"{user.username}"}}else{span {{user.username.chars().next().unwrap_or('?').to_uppercase().to_string()}}}}
                    div {class:"users_identity",strong {"{user.username}"} small {"{user.full_name} · {user.role} · " if user.active==1{"Active"}else{"Inactive"}}}
                    div {class:"users_row_actions",button {onclick:{let user=user.clone();move |_|{deleting.set(false);editor.set(Some(user.clone()));}},crate::icons::ActionLabel { label:"Edit" }} button {class:"users_delete",onclick:{let user=user.clone();move |_|{deleting.set(true);editor.set(Some(user.clone()));}},crate::icons::ActionLabel { label:"Delete" }}}
                }}
            }
        }
        if let Some(user)=editor(){UserEditor {user,delete:deleting(),db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |_|{editor.set(None);data.restart();}}}
    }}
}
#[component]
fn UserEditor(
    user: User,
    delete: bool,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let mut form = use_signal(|| user);
    let mut password = use_signal(String::new);
    let mut admin = use_signal(String::new);
    let mut secret = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {div {class:"modal_backdrop",section {class:"customer_dialog user_editor",role:"dialog",aria_modal:"true",aria_label:"User",
        h2 {if delete{"Delete user?"}else if form().id==0{"Add user"}else{"Edit user"}}
        if delete {p {"{form().username}"}} else {
            div {class:"customer_form",
                label {"Username" input {disabled:busy(),value:form().username,oninput:move |e|form.write().username=e.value()}}
                label {"Full name" input {disabled:busy(),value:form().full_name,oninput:move |e|form.write().full_name=e.value()}}
                label {if form().id==0{"Password"}else{"New password (optional)"} input {r#type:"password",autocomplete:"new-password",disabled:busy(),value:"{password}",oninput:move |e|password.set(e.value())}}
                label {"Role" select {disabled:busy(),value:form().role,onchange:move |e|form.write().role=e.value(),for role in ["Admin","Manager","Cashier"]{option {value:role,selected:form().role.eq_ignore_ascii_case(role),"{role}"}}}}
                label {"Active" input {r#type:"checkbox",disabled:busy(),checked:form().active==1,onchange:move |e|form.write().active=if e.checked(){1}else{0}}}
            }
        }
        h3 {"Administrator authorization"}
        div {class:"customer_form",label {"Admin username" input {disabled:busy(),autocomplete:"off",value:"{admin}",oninput:move |e|admin.set(e.value())}} label {"Admin password" input {r#type:"password",disabled:busy(),autocomplete:"off",value:"{secret}",oninput:move |e|secret.set(e.value())}}}
        if !error().is_empty(){p {role:"alert","{error}"}}
        div {class:"customers_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }} button {disabled:busy(),onclick:move |_|{if busy(){return;}let u=form();let pass=password();let username=admin();let credential=secret();let source=db_form.clone();busy.set(true);spawn(async move {let result=async {users::save(&connect(&source.database_config()?).await?,&u,&pass,delete,&username,&credential).await}.await;busy.set(false);secret.set(String::new());match result {Ok(())=>on_saved.call(()),Err(e)=>error.set(format!("{e:#}"))}});},if busy(){"Saving..."}else if delete{"Delete user"}else{"Save user"}}}
    }}}
}
