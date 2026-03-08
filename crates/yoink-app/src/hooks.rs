use leptos::prelude::*;

/// Set the document's `<title>` on the client side.
///
/// On SSR this is a no-op — the shell always renders `<title>yoink</title>`.
/// Once hydrated, the title updates immediately.
pub fn set_page_title(title: &str) {
    #[cfg(feature = "hydrate")]
    {
        let full = if title.is_empty() {
            "yoink".to_string()
        } else {
            format!("{title} · yoink")
        };
        if let Ok(doc) = leptos::prelude::document().query_selector("title")
            && let Some(el) = doc
        {
            el.set_text_content(Some(&full));
        }
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = title;
    }
}

/// SSE connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseStatus {
    Connected,
    Reconnecting,
}

/// Provide SSE version + status signals via Leptos context.
/// Call once from the top-level `App` component.
pub fn provide_sse_version(enabled: bool) {
    let (version, set_version, status, set_status) = create_sse_signals();
    provide_context(SseVersion(version));
    provide_context(SseConnectionStatus(status));

    #[cfg(feature = "hydrate")]
    provide_context(SseRuntimeState {
        enabled,
        started: RwSignal::new(false),
        set_version,
        set_status,
    });

    #[cfg(not(feature = "hydrate"))]
    let _ = (set_version, set_status);

    #[cfg(not(feature = "hydrate"))]
    let _ = enabled;
}

/// Provide the static auth-enabled flag to the hydrated app.
pub fn provide_auth_enabled() -> bool {
    let enabled = initial_auth_enabled();
    let (signal, _) = signal(enabled);
    provide_context(AuthEnabled(signal));
    enabled
}

pub fn use_auth_enabled() -> ReadSignal<bool> {
    expect_context::<AuthEnabled>().0
}

/// Read the SSE version signal from context. Panics if `provide_sse_version` wasn't called.
pub fn use_sse_version() -> ReadSignal<u64> {
    expect_context::<SseVersion>().0
}

/// Read the SSE connection status from context. Panics if `provide_sse_version` wasn't called.
pub fn use_sse_status() -> ReadSignal<SseStatus> {
    expect_context::<SseConnectionStatus>().0
}

#[derive(Clone, Copy)]
struct SseVersion(ReadSignal<u64>);

#[derive(Clone, Copy)]
struct SseConnectionStatus(ReadSignal<SseStatus>);
#[derive(Clone, Copy)]
struct AuthEnabled(ReadSignal<bool>);

#[cfg(feature = "hydrate")]
#[derive(Clone, Copy)]
struct SseRuntimeState {
    enabled: bool,
    started: RwSignal<bool>,
    set_version: WriteSignal<u64>,
    set_status: WriteSignal<SseStatus>,
}

fn create_sse_signals() -> (
    ReadSignal<u64>,
    WriteSignal<u64>,
    ReadSignal<SseStatus>,
    WriteSignal<SseStatus>,
) {
    #[cfg(not(feature = "hydrate"))]
    let (version, set_version) = signal(0u64);

    #[cfg(feature = "hydrate")]
    let (version, set_version) = signal(0u64);

    #[cfg(not(feature = "hydrate"))]
    let (status, set_status) = signal(SseStatus::Connected);

    #[cfg(feature = "hydrate")]
    let (status, set_status) = signal(SseStatus::Connected);

    (version, set_version, status, set_status)
}

/// Renders nothing, but starts SSE once the router navigates to an allowed page.
#[component]
pub fn SseRuntime() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        let location = leptos_router::hooks::use_location();
        let runtime = expect_context::<SseRuntimeState>();

        Effect::new(move |_| {
            let pathname = location.pathname.get();
            if runtime.enabled && should_connect_sse(pathname.as_str()) && !runtime.started.get() {
                runtime.started.set(true);
                setup_event_source(runtime.set_version, runtime.set_status);
            }
        });
    }
}

/// Wire up an `EventSource` with auto-reconnect.
///
/// We intentionally do NOT register on_cleanup here because the SSE connection
/// is meant to live for the entire app lifetime (provided once at the root).
/// The browser will close it when the page unloads.
#[cfg(feature = "hydrate")]
fn setup_event_source(set_version: WriteSignal<u64>, set_status: WriteSignal<SseStatus>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::spawn_local;

    // We use a simple recursive approach: connect(), and on error close
    // the old EventSource and schedule a reconnect after 3 seconds.
    fn connect(set_version: WriteSignal<u64>, set_status: WriteSignal<SseStatus>) {
        use std::cell::Cell;
        use std::rc::Rc;

        let es =
            Rc::new(web_sys::EventSource::new("/api/events").expect("EventSource::new failed"));

        // on open -> connected
        {
            let cb = Closure::<dyn Fn(web_sys::Event)>::new(move |_: web_sys::Event| {
                set_status.set(SseStatus::Connected);
            });
            es.set_onopen(Some(cb.as_ref().unchecked_ref()));
            cb.forget();
        }

        // on "update" event -> debounce then bump version
        //
        // During downloads the server fires SSE events per-track, which can
        // mean dozens of events per second. Without debouncing, every event
        // triggers a full resource refetch on every mounted page, causing
        // visible flashing. We coalesce rapid-fire events into a single
        // version bump after a 300ms quiet period.
        {
            let pending_timer: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
            let cb = Closure::<dyn Fn(web_sys::Event)>::new(move |_: web_sys::Event| {
                // Cancel any pending debounce timer
                if let Some(id) = pending_timer.get() {
                    leptos::prelude::window().clear_timeout_with_handle(id);
                }
                // Schedule a new version bump after 300ms of quiet
                let pt = pending_timer.clone();
                let flush = Closure::once_into_js(move || {
                    pt.set(None);
                    set_version.update(|v| *v += 1);
                });
                if let Ok(id) = leptos::prelude::window()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        flush.as_ref().unchecked_ref(),
                        300,
                    )
                {
                    pending_timer.set(Some(id));
                }
            });
            es.add_event_listener_with_callback("update", cb.as_ref().unchecked_ref())
                .expect("addEventListener failed");
            cb.forget();
        }

        // on error -> close this ES to stop the browser's auto-reconnect,
        // set status to reconnecting, then schedule our own reconnect after 3s.
        {
            let es_ref = es.clone();
            let cb = Closure::<dyn Fn(web_sys::Event)>::new(move |_: web_sys::Event| {
                // Close the EventSource immediately to prevent the browser's
                // built-in auto-reconnect from firing additional error events.
                es_ref.close();

                let set_status_inner = set_status;
                let set_version_inner = set_version;
                spawn_local(async move {
                    if auth_redirect_if_needed().await {
                        return;
                    }

                    set_status_inner.set(SseStatus::Reconnecting);

                    let reconnect_cb = Closure::once_into_js(move || {
                        connect(set_version_inner, set_status_inner);
                    });
                    let _ = leptos::prelude::window()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(
                            reconnect_cb.as_ref().unchecked_ref(),
                            3_000,
                        );
                });
            });
            es.set_onerror(Some(cb.as_ref().unchecked_ref()));
            cb.forget();
        }

        // The Rc<EventSource> is kept alive by the closures registered above
        // (which are leaked via .forget()). It lives for the app lifetime or
        // until the error handler closes it and a new one is created.
    }

    connect(set_version, set_status);
}

#[cfg(feature = "hydrate")]
async fn auth_redirect_if_needed() -> bool {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let Ok(resp_value) =
        JsFuture::from(leptos::prelude::window().fetch_with_str("/api/auth/status")).await
    else {
        return false;
    };
    let Some(resp) = resp_value.dyn_ref::<web_sys::Response>() else {
        return false;
    };

    if resp.status() != 401 {
        return false;
    }

    let pathname = leptos::prelude::window()
        .location()
        .pathname()
        .unwrap_or_else(|_| "/".to_string());
    let search = leptos::prelude::window()
        .location()
        .search()
        .unwrap_or_default();
    let next = encode_query_component(&format!("{pathname}{search}"));
    let _ = leptos::prelude::window()
        .location()
        .set_href(&format!("/login?next={next}"));
    true
}

#[cfg(feature = "hydrate")]
fn should_connect_sse(pathname: &str) -> bool {
    !matches!(
        pathname,
        "/login" | "/setup/password" | "/settings/security"
    )
}

#[cfg(feature = "hydrate")]
fn encode_query_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn initial_auth_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return leptos::prelude::document()
            .get_element_by_id("yoink-auth-state")
            .and_then(|el| el.get_attribute("data-enabled"))
            .map(|value| value == "true")
            .unwrap_or(true);
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "ssr"))]
    {
        return use_context::<yoink_shared::ServerContext>()
            .map(|ctx| ctx.auth_enabled)
            .unwrap_or(true);
    }

    #[allow(unreachable_code)]
    true
}
