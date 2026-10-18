use dioxus::prelude::*;

/// The resource cancels its previous timer when the input changes or the page unmounts.
pub fn use_debounced_search(query: Signal<String>) -> Signal<String> {
    let mut settled = use_signal(String::new);
    use_resource(move || {
        let value = query();
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if *settled.peek() != value {
                settled.set(value);
            }
        }
    });
    settled
}

/// Subscribe to the result even while pending: Resource::finished() only peeks
/// at state and does not register a reactive dependency.
pub fn use_resource_completion<T: Clone + 'static>(resource: Resource<T>, mut on_ready: impl FnMut(T) + 'static) {
    use_effect(move || {
        let snapshot=resource.read();
        if resource.finished() {
            if let Some(value)=snapshot.as_ref() { on_ready(value.clone()); }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy)]
    struct SearchState { input: Signal<String>, settled: Signal<String> }

    fn probe() -> Element {
        let input = use_signal(String::new);
        let settled = use_debounced_search(input);
        use_context_provider(move || SearchState { input, settled });
        rsx! { span { "{settled}" } }
    }

    async fn pump(dom: &mut VirtualDom, duration: Duration) {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            tokio::select! {
                _ = dom.wait_for_work() => { dom.render_immediate_to_vec(); }
                _ = tokio::time::sleep_until(end.into()) => break,
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rapid_typing_only_settles_latest_query_and_clear_search() {
        let mut dom = VirtualDom::new(probe);
        dom.rebuild_in_place();
        let mut state = dom.in_scope(ScopeId::APP, consume_context::<SearchState>);
        state.input.set("old".into());
        pump(&mut dom, Duration::from_millis(160)).await;
        assert!(state.settled.peek().is_empty());
        state.input.set("latest".into());
        pump(&mut dom, Duration::from_millis(180)).await;
        assert!(state.settled.peek().is_empty(), "old timer must be cancelled");
        pump(&mut dom, Duration::from_millis(160)).await;
        assert_eq!(&*state.settled.peek(), "latest");
        state.input.set(String::new());
        pump(&mut dom, Duration::from_millis(340)).await;
        assert!(state.settled.peek().is_empty());
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    use std::time::{Duration,Instant};
    #[derive(Clone,Copy)]
    struct State {selected:Signal<Option<i32>>,loaded:Signal<Option<Result<i32,String>>>}
    fn probe()->Element {
        let mut selected=use_signal(||None::<i32>);
        let mut loaded=use_signal(||None::<Result<i32,String>>);
        let resource=use_resource(move ||{
            let location=selected();
            async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                if location==Some(9) {Err("Catalog unavailable".into())}else{Ok(location.unwrap_or(1))}
            }
        });
        use_resource_completion(resource,move |result|{
            if let Ok(id)=&result {if *selected.peek()!=Some(*id){selected.set(Some(*id));}}
            loaded.set(Some(result));
        });
        use_context_provider(move ||State{selected,loaded});
        rsx!{span {"Catalog"}}
    }
    async fn pump(dom:&mut VirtualDom) {
        let end=Instant::now()+Duration::from_millis(150);
        while Instant::now()<end {
            tokio::select! {_=dom.wait_for_work()=>{dom.render_immediate_to_vec();},_=tokio::time::sleep_until(end.into())=>break}
        }
    }
    #[tokio::test(flavor="current_thread")]
    async fn initial_load_selection_changes_and_errors_complete_without_another_click() {
        let mut dom=VirtualDom::new(probe);dom.rebuild_in_place();
        let mut state=dom.in_scope(ScopeId::APP,consume_context::<State>);
        pump(&mut dom).await;
        assert_eq!(*state.loaded.peek(),Some(Ok(1)));
        state.selected.set(Some(2));dom.render_immediate_to_vec();
        assert_eq!(*state.selected.peek(),Some(2),"old catalog must not reset the new location");
        pump(&mut dom).await;
        assert_eq!(*state.loaded.peek(),Some(Ok(2)));
        state.selected.set(Some(3));dom.render_immediate_to_vec();
        state.selected.set(Some(4));pump(&mut dom).await;
        assert_eq!(*state.loaded.peek(),Some(Ok(4)));
        state.selected.set(Some(9));pump(&mut dom).await;
        assert_eq!(*state.loaded.peek(),Some(Err("Catalog unavailable".into())));
    }
}
