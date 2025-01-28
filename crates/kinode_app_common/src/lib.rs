
use kinode_process_lib::logging::info;
use kinode_process_lib::logging::init_logging;
use kinode_process_lib::logging::Level;
use kinode_process_lib::http::server::WsMessageType;
use std::any::Any;
use std::collections::HashMap;
use std::mem::replace;
use std::sync::Mutex;


use once_cell::sync::Lazy;

use kinode_process_lib::{
    await_message, get_typed_state, homepage, http, kiprintln, set_state, LazyLoadBlob, Message,
    SendError, SendErrorKind,
};

/// We store the user's state as `dyn Any + Send`, plus the callback map.
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

/// A single global that holds the entire application state
pub static GLOBAL_APP_STATE: Lazy<Mutex<Option<AppState>>> = Lazy::new(|| Mutex::new(None));

#[macro_export]
macro_rules! timer {
    ($duration:expr, ($st:ident : $user_state_ty:ty) $callback_block:block) => {{
        use uuid::Uuid; // Make sure `uuid` crate is in scope

        // 1) Generate a correlation_id
        let correlation_id = Uuid::new_v4().to_string();

        // 2) Insert the callback into the global callback map
        {
            let mut guard = $crate::GLOBAL_APP_STATE.lock().unwrap();
            if let Some(app_state_any) = guard.as_mut() {
                app_state_any.pending_callbacks.insert(
                    correlation_id.clone(),
                    $crate::PendingCallback {
                        // This callback runs when the timer response arrives
                        on_success: Box::new(move |_resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                            let $st = any_state
                                .downcast_mut::<$user_state_ty>()
                                .ok_or_else(|| anyhow::anyhow!("Downcast failed!"))?;
                            $callback_block
                            Ok(())
                        }),
                        // No timeout callback
                        on_timeout: None,
                    },
                );
            }
        }

        // 3) Construct and send the request
        let total_timeout_seconds = ($duration / 1000) + 1;
        let mut request = kinode_process_lib::Request::to(("our", "timer", "distro", "sys"))
            .body(TimerAction::SetTimer($duration))
            .expects_response(total_timeout_seconds);

        // 4) Attach correlation_id so the framework can match the response
        request = request.context(correlation_id.as_bytes());

        // 5) Fire it off
        request.send().unwrap();
    }};
}


#[macro_export]
macro_rules! send {
    // ---------------------------------------------------------------------
    // 1) Original version (no explicit timeout, no on_timeout)
    // ---------------------------------------------------------------------
    (
        $destination:expr,
        $body:expr,
        ($resp:ident, $st:ident : $user_state_ty:ty) $callback_block:block
    ) => {
        $crate::send!($destination, $body, ($resp, $st: $user_state_ty) $callback_block, 30);
    };

    // ---------------------------------------------------------------------
    // 2) Version with explicit timeout, but no on_timeout block
    // ---------------------------------------------------------------------
    (
        $destination:expr,
        $body:expr,
        ($resp:ident, $st:ident : $user_state_ty:ty) $callback_block:block,
        $timeout:expr
    ) => {{
        // Insert a callback with no on_timeout
        let correlation_id = uuid::Uuid::new_v4().to_string();
        {
            let mut guard = $crate::GLOBAL_APP_STATE.lock().unwrap();
            if let Some(app_state_any) = guard.as_mut() {
                app_state_any.pending_callbacks.insert(
                    correlation_id.clone(),
                    $crate::PendingCallback {
                        on_success: Box::new(move |resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                            // success
                            let $resp = serde_json::from_slice(resp_bytes).map_err(|e| {
                                anyhow::anyhow!("Failed to deserialize response: {}", e)
                            })?;
                            let $st = any_state.downcast_mut::<$user_state_ty>().ok_or_else(|| {
                                anyhow::anyhow!("Downcast failed!")
                            })?;
                            $callback_block
                            Ok(())
                        }),
                        on_timeout: None, // no custom timeout callback
                    },
                );
            }
        }

        // Send the request with $timeout
        let _ = kinode_process_lib::Request::to($destination)
            .context(correlation_id.as_bytes())
            .body($body)
            .expects_response($timeout)
            .send();
    }};

    // ---------------------------------------------------------------------
    // 3) Version with explicit timeout AND an on_timeout block
    // ---------------------------------------------------------------------
    (
        $destination:expr,
        $body:expr,
        ($resp:ident, $st:ident : $user_state_ty:ty) $callback_block:block,
        $timeout:expr,
        on_timeout => $timeout_block:block
    ) => {{
        let correlation_id = uuid::Uuid::new_v4().to_string();
        {
            let mut guard = $crate::GLOBAL_APP_STATE.lock().unwrap();
            if let Some(app_state_any) = guard.as_mut() {
                app_state_any.pending_callbacks.insert(
                    correlation_id.clone(),
                    $crate::PendingCallback {
                        // ========== On Success ==========
                        on_success: Box::new(move |resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                            let $resp = serde_json::from_slice(resp_bytes).map_err(|e| {
                                anyhow::anyhow!("Failed to deserialize response: {}", e)
                            })?;
                            let $st = any_state.downcast_mut::<$user_state_ty>().ok_or_else(|| {
                                anyhow::anyhow!("Downcast failed!")
                            })?;
                            $callback_block
                            Ok(())
                        }),
                        // ========== On Timeout ==========
                        on_timeout: Some(Box::new(move |any_state: &mut dyn std::any::Any| {
                            let $st = any_state
                                .downcast_mut::<$user_state_ty>()
                                .ok_or_else(|| anyhow::anyhow!("Downcast failed!"))?;
                            $timeout_block
                            Ok(())
                        })),
                    },
                );
            }
        }

        let _ = kinode_process_lib::Request::to($destination)
            .context(correlation_id.as_bytes())
            .body($body)
            .expects_response($timeout)
            .send();
    }};
}

/// Trait that must be implemented by application state types
pub trait State {
    /// Creates a new instance of the state.
    fn new() -> Self;
}

/// Creates a standard Kinode application with HTTP server and WebSocket support
pub fn app<S, T1, T2, T3>(
    app_name: &str,
    app_icon: Option<&str>,
    app_widget: Option<&str>,
    handle_api_call: impl Fn(&mut S, T1) -> (http::server::HttpResponse, Vec<u8>),
    handle_local_request: impl Fn(&Message, &mut S, &mut http::server::HttpServer, T2),
    handle_remote_request: impl Fn(&Message, &mut S, &mut http::server::HttpServer, T3),
    handle_ws: impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
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
        let mut server = http::server::HttpServer::new(5);

        if let Err(e) = server.serve_ui("ui", vec!["/"], http::server::HttpBindingConfig::default())
        {
            panic!("failed to serve UI: {e}");
        }

        server
            .bind_http_path("/api", http::server::HttpBindingConfig::default())
            .expect("failed to serve API path");

        server
            .bind_ws_path("/updates", http::server::WsBindingConfig::default())
            .expect("failed to bind WS path");

        // 1) Load or init the typed state - now with graceful fallback
        let existing =
            get_typed_state(|bytes| rmp_serde::from_slice::<S>(bytes)).unwrap_or_else(|| S::new());

        // 2) Put the user's S into the global as Box<dyn Any>
        {
            let mut guard = GLOBAL_APP_STATE.lock().unwrap();
            *guard = Some(AppState::new(existing));
        }

        // 3) main loop
        loop {
            match await_message() {
                // -------------------------------------------------------
                // Handle SendError (timeout, disconnected, etc.)
                // -------------------------------------------------------
                Err(send_error) => {
                    pretty_print_send_error(&send_error);

                    // We'll extract the correlation_id from send_error.context()
                    let correlation_id = send_error
                        .context
                        .as_ref()
                        .map(|bytes| String::from_utf8_lossy(bytes).to_string());

                    let mut maybe_state: Option<S> = None;
                    let mut maybe_timeout_callback: Option<
                        Box<dyn FnOnce(&mut dyn Any) -> anyhow::Result<()> + Send>,
                    > = None;

                    {
                        let mut guard = GLOBAL_APP_STATE.lock().unwrap();
                        if let Some(app_state) = guard.as_mut() {
                            // Extract user state S by "taking it out"
                            let original_box =
                                std::mem::replace(&mut app_state.user_state, Box::new(()));
                            if original_box.is::<S>() {
                                if let Ok(real_s_box) = original_box.downcast::<S>() {
                                    let s = *real_s_box;
                                    maybe_state = Some(s);
                                }
                            } else {
                                app_state.user_state = original_box;
                            }

                            // If there's a correlation_id, see if there's a matching callback
                            if let Some(cid) = &correlation_id {
                                if let Some(pending) = app_state.pending_callbacks.remove(cid) {
                                    if matches!(send_error.kind, SendErrorKind::Timeout) {
                                        maybe_timeout_callback = pending.on_timeout;
                                    }
                                }
                            }
                        }
                    }

                    // If we have a user-defined on_timeout callback, run it
                    if let Some(ref mut state) = maybe_state {
                        if let Some(cb) = maybe_timeout_callback {
                            if let Err(e) = cb(state as &mut dyn Any) {
                                kiprintln!("Error in on_timeout callback: {e}");
                            }
                        }
                    }

                    // put S back and persist
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

/// -------------- 3) The "app!" macros for exporting  ----------------
#[macro_export]
macro_rules! erect {
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                let init = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $f1,
                    |_, _, _, _: ()| {},
                    $f2,
                    |_, _, _, _| {}, // no-op ws handler
                );
                init();
            }
        }
        export!(Component);
    };
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident, $f3:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                let init = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $f1,
                    $f2,
                    |_, _, _, _: ()| {},
                    $f3,
                );
                init();
            }
        }
        export!(Component);
    };
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident, $f3:ident, $f4:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                let init = $crate::app($app_name, $app_icon, $app_widget, $f1, $f2, $f3, $f4);
                init();
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
