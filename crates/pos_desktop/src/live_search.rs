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
