use kinode_process_lib::get_state;
use kinode_process_lib::http::server::WsMessageType;
use kinode_process_lib::logging::info;
use kinode_process_lib::logging::init_logging;
use kinode_process_lib::logging::warn;
use kinode_process_lib::logging::Level;
use std::any::Any;
use std::collections::HashMap;
use std::mem::replace;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use kinode_process_lib::{
    await_message, homepage, http, kiprintln, set_state, LazyLoadBlob, Message, SendError,
    SendErrorKind,
};

/// The application state containing the callback map plus the user state. This is the actual state.
pub struct AppState {
    pub user_state: Box<dyn Any + Send>,
    pub pending_callbacks: HashMap<String, PendingCallback>,
}

pub struct PendingCallback {
    pub on_success: Box<dyn FnOnce(&[u8], &mut dyn Any) -> anyhow::Result<()> + Send>,
    pub on_timeout: Option<Box<dyn FnOnce(&mut dyn Any) -> anyhow::Result<()> + Send>>,
}

impl AppState {
    pub fn new<U: Any + Send>(user_state: U) -> Self {
        Self {
            user_state: Box::new(user_state),
            pending_callbacks: HashMap::new(),
        }
    }
}

pub static GLOBAL_APP_STATE: Lazy<Mutex<Option<AppState>>> = Lazy::new(|| Mutex::new(None));

#[macro_export]
macro_rules! timer {
    ($duration:expr, ($st:ident : $user_state_ty:ty) $callback_block:block) => {{
        use uuid::Uuid;

        let correlation_id = Uuid::new_v4().to_string();

        {
            let mut guard = $crate::GLOBAL_APP_STATE.lock().unwrap();
            if let Some(app_state_any) = guard.as_mut() {
                app_state_any.pending_callbacks.insert(
                    correlation_id.clone(),
                    $crate::PendingCallback {
                        on_success: Box::new(move |_resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                            let $st = any_state
                                .downcast_mut::<$user_state_ty>()
                                .ok_or_else(|| anyhow::anyhow!("Downcast failed!"))?;
                            $callback_block
                            Ok(())
                        }),
                        on_timeout: None,
                    },
                );
            }
        }

        let total_timeout_seconds = ($duration / 1000) + 1;
        let _ = kinode_process_lib::Request::to(("our", "timer", "distro", "sys"))
            .body(TimerAction::SetTimer($duration))
            .expects_response(total_timeout_seconds)
            .context(correlation_id.as_bytes())
            .send();
    }};
}

/// Trait that must be implemented by application state types
pub trait State {
    /// Creates a new instance of the state.
    fn new() -> Self;
}

/// Initialize state from persisted storage or create new if none exists
fn initialize_state<S>() -> S
where
    S: State + for<'de> serde::Deserialize<'de>,
{
    match get_state() {
        Some(bytes) => match rmp_serde::from_slice::<S>(&bytes) {
            Ok(state) => state,
            Err(e) => {
                panic!("error deserializing existing state: {e}. We're panicking because we don't want to nuke state by setting it to a new instance.");
            }
        },
        None => {
            info!("no existing state, creating new one");
            S::new()
        }
    }
}

fn setup_server(
    ui_config: &http::server::HttpBindingConfig,
    api_config: &http::server::HttpBindingConfig,
    ws_config: &http::server::WsBindingConfig,
) -> http::server::HttpServer {
    let mut server = http::server::HttpServer::new(5);

    if let Err(e) = server.serve_ui("ui", vec!["/"], ui_config.clone()) {
        panic!("failed to serve UI: {e}");
    }

    server
        .bind_http_path("/api", api_config.clone())
        .expect("failed to serve API path");

    server
        .bind_ws_path("/updates", ws_config.clone())
        .expect("failed to bind WS path");

    server
}

pub fn app<S, T1, T2, T3>(
    app_name: &str,
    app_icon: Option<&str>,
    app_widget: Option<&str>,
    ui_config: http::server::HttpBindingConfig,
    api_config: http::server::HttpBindingConfig,
    ws_config: http::server::WsBindingConfig,
    handle_api_call: impl Fn(&mut S, T1) -> (http::server::HttpResponse, Vec<u8>),
    handle_local_request: impl Fn(&Message, &mut S, &mut http::server::HttpServer, T2),
    handle_remote_request: impl Fn(&Message, &mut S, &mut http::server::HttpServer, T3),
    handle_ws: impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
    init_fn: fn(&mut S),
) -> impl Fn()
where
    S: State + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    T1: serde::Serialize + serde::de::DeserializeOwned,
    T2: serde::Serialize + serde::de::DeserializeOwned,
    T3: serde::Serialize + serde::de::DeserializeOwned,
{
    if app_icon.is_some() && app_widget.is_some() {
        homepage::add_to_homepage(app_name, app_icon, Some("/"), app_widget);
    }

    move || {
        init_logging(Level::DEBUG, Level::INFO, None, Some((0, 0, 1, 1)), None).unwrap();
        info!("starting app");

        let mut server = setup_server(&ui_config, &api_config, &ws_config);
        let existing = initialize_state::<S>();

        // TODO: Make general func?
        // Initialize the global and user state
        let mut binding = GLOBAL_APP_STATE.lock().unwrap();
        *binding = Some(AppState::new(existing));
        let mut global_state = binding.as_mut().unwrap();
        let mut user_state = global_state.user_state.downcast_mut::<S>().unwrap();

        // Execute the user specified init function
        init_fn(&mut user_state);

        loop {
            match await_message() {
                Err(send_error) => {
                    pretty_print_send_error(&send_error);
                    let Some(correlation_id) = send_error
                        .context
                        .as_ref()
                        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                    else {
                        continue;
                    };

                    let Some(pending) = global_state.pending_callbacks.remove(&correlation_id)
                    else {
                        warn!("No pending callback found for correlation id: {correlation_id}. This should never happen.");
                        continue;
                    };
                    if let Some(cb) = pending.on_timeout {
                        if let Err(e) = cb(user_state as &mut dyn Any) {
                            kiprintln!("Error in on_timeout callback: {e}");
                        }
                    }

                    // TODO: Zena: PERSIST THE STATE
                    if let Ok(s_bytes) = rmp_serde::to_vec(&user_state) {
                        set_state(&s_bytes);
                    }
                }

                // -------------------------------------------------------
                // Otherwise, normal message
                // -------------------------------------------------------
                Ok(message) => {
                    let mut maybe_callback: Option<
                        Box<dyn FnOnce(&[u8], &mut dyn Any) -> anyhow::Result<()> + Send>,
                    > = None;
                    let mut maybe_state: Option<S> = None;

                    {
                        // Lock briefly to check if there's a matching callback
                        let correlation_id = message
                            .context()
                            .map(|c| String::from_utf8_lossy(c).to_string());

                        let mut guard = GLOBAL_APP_STATE.lock().unwrap();

                        if let Some(app_state) = guard.as_mut() {
                            if let Some(cid) = correlation_id {
                                if let Some(pending) = app_state.pending_callbacks.remove(&cid) {
                                    // We only want the on_success closure here:
                                    maybe_callback = Some(pending.on_success);

                                    // Take the user_state out of the struct
                                    let original_box =
                                        replace(&mut app_state.user_state, Box::new(()));

                                    // Try to downcast to S
                                    if original_box.is::<S>() {
                                        if let Ok(real_s_box) = original_box.downcast::<S>() {
                                            let s = *real_s_box;
                                            maybe_state = Some(s);
                                        }
                                    } else {
                                        // Put back the original box since we know it's not our type
                                        app_state.user_state = original_box;
                                    }
                                }
                            }
                        }
                    } // lock dropped

                    // (B) If we found a callback, run it outside the lock
                    if let Some(callback_fn) = maybe_callback {
                        if let Some(ref mut s) = maybe_state {
                            if let Err(e) = callback_fn(message.body(), s as &mut dyn Any) {
                                kiprintln!("Error in callback: {e}");
                            }
                        }

                        // Put the state back & do set_state
                        {
                            let mut guard = GLOBAL_APP_STATE.lock().unwrap();
                            if let Some(app_state) = guard.as_mut() {
                                if let Some(s) = maybe_state.take() {
                                    app_state.user_state = Box::new(s);
                                }
                                if let Some(state_ref) = app_state.user_state.downcast_ref::<S>() {
                                    if let Ok(s_bytes) = rmp_serde::to_vec(state_ref) {
                                        let _ = set_state(&s_bytes);
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    // (C) Otherwise not a callback: local or remote
                    let mut state_opt: Option<S> = None;
                    let mut is_local_msg = false;
                    let mut from_http_server = false;

                    // Lock to "take" the user state
                    {
                        let mut guard = GLOBAL_APP_STATE.lock().unwrap();
                        if let Some(app_state) = guard.as_mut() {
                            let original_box = replace(&mut app_state.user_state, Box::new(()));
                            match original_box.downcast::<S>() {
                                Ok(real_s_box) => {
                                    let s = *real_s_box;
                                    state_opt = Some(s);
                                    is_local_msg = message.is_local();
                                    from_http_server =
                                        message.source().process == "http-server:distro:sys";
                                }
                                Err(unboxed_any) => {
                                    // Put it back if downcast fails
                                    app_state.user_state = unboxed_any;
                                }
                            }
                        }
                    }

                    // (D) Call user handlers outside the lock
                    if let Some(ref mut state) = state_opt {
                        if is_local_msg {
                            if from_http_server {
                                http_request(
                                    &message,
                                    state,
                                    &mut server,
                                    &handle_api_call,
                                    &handle_ws,
                                );
                            } else {
                                local_request(&message, state, &mut server, &handle_local_request);
                            }
                        } else {
                            remote_request(&message, state, &mut server, &handle_remote_request);
                        }
                    }

                    // (E) Put S back and set_state
                    {
                        let mut guard = GLOBAL_APP_STATE.lock().unwrap();
                        if let Some(app_state) = guard.as_mut() {
                            if let Some(s) = state_opt.take() {
                                app_state.user_state = Box::new(s);
                            }
                            if let Some(state_ref) = app_state.user_state.downcast_ref::<S>() {
                                if let Ok(s_bytes) = rmp_serde::to_vec(state_ref) {
                                    let _ = set_state(&s_bytes);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn http_request<S, T1>(
    message: &Message,
    state: &mut S,
    server: &mut http::server::HttpServer,
    handle_api_call: impl Fn(&mut S, T1) -> (http::server::HttpResponse, Vec<u8>),
    handle_ws: impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
) where
    T1: serde::Serialize + serde::de::DeserializeOwned,
{
    let http_request = serde_json::from_slice::<http::server::HttpServerRequest>(message.body())
        .expect("failed to parse HTTP request");

    let state_ptr: *mut S = state;
    let server_ptr: *mut http::server::HttpServer = server;

    server.handle_request(
        http_request,
        move |_incoming| {
            let state_ref: &mut S = unsafe { &mut *state_ptr };

            let response = http::server::HttpResponse::new(200 as u16);
            let Some(blob) = message.blob() else {
                return (response.set_status(400), None);
            };
            let Ok(call) = serde_json::from_slice::<T1>(blob.bytes()) else {
                return (response.set_status(400), None);
            };

            let (response, bytes) = handle_api_call(state_ref, call);

            (
                response,
                Some(LazyLoadBlob::new(Some("application/json"), bytes)),
            )
        },
        move |channel_id, msg_type, blob| {
            let state_ref: &mut S = unsafe { &mut *state_ptr };
            let server_ref: &mut http::server::HttpServer = unsafe { &mut *server_ptr };

            handle_ws(state_ref, server_ref, channel_id, msg_type, blob);
        },
    );
}

fn local_request<S, T>(
    message: &Message,
    state: &mut S,
    server: &mut http::server::HttpServer,
    handle_local_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T),
) where
    S: std::fmt::Debug,
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let Ok(request) = serde_json::from_slice::<T>(message.body()) else {
        if message.body() == b"debug" {
            kiprintln!("state:\n{:#?}", state);
        }
        return;
    };
    handle_local_request(message, state, server, request);
}

fn remote_request<S, T>(
    message: &Message,
    state: &mut S,
    server: &mut http::server::HttpServer,
    handle_remote_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T),
) where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let Ok(request) = serde_json::from_slice::<T>(message.body()) else {
        return;
    };
    handle_remote_request(message, state, server, request);
}

/// Pretty prints a SendError in a more readable format
fn pretty_print_send_error(error: &SendError) {
    let kind = &error.kind;
    let target = &error.target;

    // Try to decode body as UTF-8 string, fall back to showing as bytes
    let body = String::from_utf8(error.message.body().to_vec())
        .map(|s| format!("\"{}\"", s))
        .unwrap_or_else(|_| format!("{:?}", error.message.body()));

    // Try to decode context as UTF-8 string
    let context = error
        .context
        .as_ref()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned());

    kiprintln!(
        "SendError {{
    kind: {:?},
    target: {},
    body: {},
    context: {}
}}",
        kind,
        target,
        body,
        context
            .map(|s| format!("\"{}\"", s))
            .unwrap_or("None".to_string())
    );
}

/// Creates and exports your Kinode microservice with all standard boilerplate.
///
/// **Parameters**:
/// - `$app_name`: &str — The display name of your app.
/// - `$app_icon`: Option<&str> — Icon path or `None`.
/// - `$app_widget`: Option<&str> — Widget path or `None`.
/// - `$ui_config`: `HttpBindingConfig` — config for the UI (pass `.default()` if not needed).
/// - `$api_config`: `HttpBindingConfig` — config for the `/api` endpoint.
/// - `$ws_config`: `WsBindingConfig` — config for the `/updates` path.
/// - `$handle_api_call`, `$handle_local_request`, `$handle_remote_request`, `$handle_ws`:
///   the 4 request-handling functions.
/// - `$init_fn`: function `fn(&mut S)` called after setup, before the main loop.
///
/// **Example**:
///
/// ```ignore
/// use kinode_process_lib::http::server::{HttpBindingConfig, WsBindingConfig};
///
/// fn handle_api_call(state: &mut MyState, req: MyRequest) -> (HttpResponse, Vec<u8>) {
///     // ...
/// }
/// fn handle_local_request(_msg: &Message, _state: &mut MyState, _srv: &mut HttpServer, _req: MyLocal) { }
/// fn handle_remote_request(_msg: &Message, _state: &mut MyState, _srv: &mut HttpServer, _req: MyRemote) { }
/// fn handle_ws(_state: &mut MyState, _srv: &mut HttpServer, _id: u32, _typ: WsMessageType, _blob: LazyLoadBlob) { }
///
/// fn my_init_fn(state: &mut MyState) {
///     // custom logic (timers, logs, etc.)
/// }
///
/// erect!(
///     "My App",
///     Some("icon.png"),
///     Some("widget_id"),
///     HttpBindingConfig::default(), // config for serving UI
///     HttpBindingConfig::default(), // config for /api
///     WsBindingConfig::default(),   // config for /updates
///     handle_api_call,
///     handle_local_request,
///     handle_remote_request,
///     handle_ws,
///     my_init_fn
/// );
/// ```
#[macro_export]
macro_rules! erect {
    (
        $app_name:expr,
        $app_icon:expr,
        $app_widget:expr,
        $ui_config:expr,
        $api_config:expr,
        $ws_config:expr,
        $handle_api_call:ident,
        $handle_local_request:ident,
        $handle_remote_request:ident,
        $handle_ws:ident,
        $init_fn:ident
    ) => {
        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                // Pass in all arguments to `app`
                let init_closure = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $ui_config,
                    $api_config,
                    $ws_config,
                    $handle_api_call,
                    $handle_local_request,
                    $handle_remote_request,
                    $handle_ws,
                    $init_fn,
                );
                init_closure();
            }
        }
        export!(Component);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_types_internal {
    (
        $(
            $outer:ident {
                $(
                    $variant:ident $req_ty:ty => $res_ty:ty
                )*
            }
        ),*
        $(,)?
    ) => {
        $crate::paste::paste! {
            #[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
            pub enum Req {
                $(
                    $outer([<$outer Request>]),
                )*
            }

            $(
                #[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
                pub enum [<$outer Request>] {
                    $(
                        $variant($req_ty),
                    )*
                }

                #[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
                pub enum [<$outer Response>] {
                    $(
                        $variant($res_ty),
                    )*
                }
            )*
        }
    };
}

#[macro_export]
macro_rules! declare_types {
    ($($tt:tt)*) => {
        $crate::__declare_types_internal! { $($tt)* }
    };
}

// Re-export paste for use in our macros
pub use paste;
