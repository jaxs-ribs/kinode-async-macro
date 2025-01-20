use std::any::Any;
use std::collections::HashMap;
use std::mem::replace;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use kinode_process_lib::{
    await_message, get_typed_state, homepage, http, kiprintln, set_state, Address, LazyLoadBlob,
    Message, SendError,
};

/// We store the user's state as `dyn Any + Send`, plus the callback map.
pub struct AppState {
    pub user_state: Box<dyn Any + Send>,
    pub pending_callbacks: HashMap<
        String, // correlation_id
        Box<dyn FnOnce(&[u8], &mut dyn Any) -> anyhow::Result<()> + Send>,
    >,
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
macro_rules! send {
    (
        $destination:expr,
        $body:expr,
        ($resp:ident, $st:ident : $user_state_ty:ty) $callback_block:block
    ) => {{
        // 1) Generate a correlation_id, insert a callback with a closure that does downcast to $user_state_ty
        let correlation_id = uuid::Uuid::new_v4().to_string();
        {
            let mut guard = $crate::GLOBAL_APP_STATE.lock().unwrap();
            if let Some(app_state_any) = guard.as_mut() {
                app_state_any.pending_callbacks.insert(
                    correlation_id.clone(),
                    Box::new(move |$resp: &[u8], any_state: &mut dyn std::any::Any| {
                        // Here’s where the macro uses $user_state_ty from the pattern:
                        let $st = any_state
                            .downcast_mut::<$user_state_ty>()
                            .ok_or_else(|| anyhow::anyhow!("Downcast failed!"))?;
                        // Then just run the user’s code block:
                        $callback_block
                        Ok(())
                    }),
                );
            }
        }

        // 2) Send the actual request
        let _ = kinode_process_lib::Request::to($destination)
            .context(correlation_id.as_bytes())
            .body($body)
            .expects_response(10)
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
    handle_send_error: impl Fn(&mut S, &mut http::server::HttpServer, SendError),
) -> impl Fn(Address)
where
    S: State + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    T1: serde::Serialize + serde::de::DeserializeOwned,
    T2: serde::Serialize + serde::de::DeserializeOwned,
    T3: serde::Serialize + serde::de::DeserializeOwned,
{
    homepage::add_to_homepage(app_name, app_icon, Some("/"), app_widget);

    move |our: Address| {
        kiprintln!("Starting app");
        let mut server = http::server::HttpServer::new(5);

        if let Err(e) = server.serve_ui(
            &our,
            "ui",
            vec!["/"],
            http::server::HttpBindingConfig::default(),
        ) {
            panic!("failed to serve UI: {e}");
        }

        server
            .bind_http_path("/api", http::server::HttpBindingConfig::default())
            .expect("failed to serve API path");

        server
            .bind_ws_path("/updates", http::server::WsBindingConfig::default())
            .expect("failed to bind WS path");

        // 1) Load or init the typed state
        let existing =
            get_typed_state(|bytes| serde_json::from_slice::<S>(bytes)).unwrap_or_else(|| {
                let state = S::new();
                set_state(&serde_json::to_vec(&state).expect("failed to serialize state to bytes"));
                state
            });

        // 2) Put the user's S into the global as Box<dyn Any>
        {
            let mut guard = GLOBAL_APP_STATE.lock().unwrap();
            *guard = Some(AppState::new(existing));
        }

        // 3) main loop
        loop {
            match await_message() {
                // If we get a SendError:
                Err(send_error) => {
                    kiprintln!("Got send_error, locking global to handle_send_error");
                    let mut guard = GLOBAL_APP_STATE.lock().unwrap();
                    if let Some(app_st) = guard.as_mut() {
                        if let Some(s) = app_st.user_state.downcast_mut::<S>() {
                            handle_send_error(s, &mut server, send_error);
                        }
                    }
                }

                // Otherwise, normal message
                Ok(message) => {
                    // (A) We'll store them in separate variables
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
                                if let Some(cb) = app_state.pending_callbacks.remove(&cid) {
                                    // Save the callback
                                    maybe_callback = Some(cb);

                                    // Take the user_state out of the struct
                                    let original_box =
                                        replace(&mut app_state.user_state, Box::new(()));

                                    // Try to downcast to S
                                    match original_box.downcast::<S>() {
                                        Ok(real_s_box) => {
                                            let s = *real_s_box;
                                            maybe_state = Some(s);
                                        }
                                        Err(unboxed_any) => {
                                            // Put it back if downcast failed
                                            app_state.user_state = unboxed_any;
                                        }
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
                                    if let Ok(s_bytes) = serde_json::to_vec(state_ref) {
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
                                    is_local_msg = message.is_local(&our);
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
                                http_request(&message, state, &mut server, &handle_api_call);
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
                                if let Ok(s_bytes) = serde_json::to_vec(state_ref) {
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

// Helper functions
fn http_request<S, T1>(
    message: &Message,
    state: &mut S,
    server: &mut http::server::HttpServer,
    handle_api_call: &impl Fn(&mut S, T1) -> (http::server::HttpResponse, Vec<u8>),
) where
    T1: serde::Serialize + serde::de::DeserializeOwned,
{
    let http_request = serde_json::from_slice::<http::server::HttpServerRequest>(&message.body())
        .expect("failed to parse HTTP request");

    server.handle_request(
        http_request,
        |_incoming| {
            let response = http::server::HttpResponse::new(200 as u16);
            let Some(blob) = message.blob() else {
                return (response.set_status(400), None);
            };
            let Ok(call) = serde_json::from_slice::<T1>(blob.bytes()) else {
                return (response.set_status(400), None);
            };

            let (response, bytes) = handle_api_call(state, call);
            (
                response,
                Some(LazyLoadBlob::new(Some("application/json"), bytes)),
            )
        },
        |_, _, _| {
            // skip ws
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
    let Ok(request) = serde_json::from_slice::<T>(&message.body()) else {
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
    let Ok(request) = serde_json::from_slice::<T>(&message.body()) else {
        return;
    };
    handle_remote_request(message, state, server, request);
}

/// -------------- 3) The "app!" macros for exporting  ----------------
// same as your original code. Now they will use the new `app()` function
#[macro_export]
macro_rules! Erect {
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(our: String) {
                let our: Address = our.parse().unwrap();
                // we pass the default T3=() for the local request, for example
                let init = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $f1,
                    |_, _, _, _: ()| {},
                    $f2,
                    |_, _, _| {},
                );
                init(our);
            }
        }
        export!(Component);
    };
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident, $f3:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(our: String) {
                let our: Address = our.parse().unwrap();
                let init = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $f1,
                    $f2,
                    $f3,
                    |_, _, _| {},
                );
                init(our);
            }
        }
        export!(Component);
    };
    ($app_name:expr, $app_icon:expr, $app_widget:expr, $f1:ident, $f2:ident, $f3:ident, $f4:ident) => {
        struct Component;
        impl Guest for Component {
            fn init(our: String) {
                let our: Address = our.parse().unwrap();
                let init = $crate::app($app_name, $app_icon, $app_widget, $f1, $f2, $f3, $f4);
                init(our);
            }
        }
        export!(Component);
    };
}
