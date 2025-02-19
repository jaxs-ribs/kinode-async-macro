use hyperware_process_lib::get_state;
use hyperware_process_lib::http::server::send_response;
use hyperware_process_lib::http::server::HttpServerRequest;
use hyperware_process_lib::http::server::WsMessageType;
use hyperware_process_lib::http::StatusCode;
use hyperware_process_lib::logging::info;
use hyperware_process_lib::logging::init_logging;
use hyperware_process_lib::logging::warn;
use hyperware_process_lib::logging::Level;
use hyperware_process_lib::Address;
use hyperware_process_lib::Request;
use hyperware_process_lib::SendErrorKind;
use serde::Deserialize;
use serde::Serialize;
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::task::noop_waker_ref;
use uuid::Uuid;

use hyperware_process_lib::{
    await_message, homepage, http, kiprintln, set_state, LazyLoadBlob, Message, SendError,
};

pub mod prelude {
    pub use crate::HIDDEN_STATE;
    // Add other commonly used items here
}

thread_local! {
    pub static HIDDEN_STATE: RefCell<Option<HiddenState>> = RefCell::new(None);
}

thread_local! {
    pub static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
}

thread_local! {
    pub static RESPONSE_REGISTRY: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

pub struct Executor {
    tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>,
}

impl Executor {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn spawn(&mut self, fut: impl Future<Output = ()> + 'static) {
        self.tasks.push(Box::pin(fut));
    }

    pub fn poll_all_tasks(&mut self) {
        let mut ctx = Context::from_waker(noop_waker_ref());
        let mut completed = Vec::new();

        for i in 0..self.tasks.len() {
            if let Poll::Ready(()) = self.tasks[i].as_mut().poll(&mut ctx) {
                completed.push(i);
            }
        }

        for idx in completed.into_iter().rev() {
            let _ = self.tasks.remove(idx);
        }
    }
}
struct ResponseFuture {
    correlation_id: String,
}

impl ResponseFuture {
    fn new(correlation_id: String) -> Self {
        Self { correlation_id }
    }
}

impl Future for ResponseFuture {
    type Output = Vec<u8>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let correlation_id = &self.correlation_id;

        let maybe_bytes = RESPONSE_REGISTRY.with(|registry| {
            let mut map = registry.borrow_mut();
            map.remove(correlation_id)
        });

        if let Some(bytes) = maybe_bytes {
            Poll::Ready(bytes)
        } else {
            Poll::Pending
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendResult<R> {
    Success(R),
    Timeout,
    Offline,
    DeserializationError(String),
}

pub async fn send<R>(
    message: impl serde::Serialize,
    target: Address,
    timeout_secs: u64,
) -> SendResult<R>
where
    R: serde::de::DeserializeOwned,
{
    let correlation_id = Uuid::new_v4().to_string();
    let body = serde_json::to_vec(&message).expect("Failed to serialize message");

    let _ = Request::to(target)
        .body(body)
        .context(correlation_id.as_bytes().to_vec())
        .expects_response(timeout_secs)
        .send();

    let response_bytes = ResponseFuture::new(correlation_id).await;
    match serde_json::from_slice(&response_bytes) {
        Ok(result) => return SendResult::Success(result),
        Err(_) => match serde_json::from_slice::<SendErrorKind>(&response_bytes) {
            Ok(kind) => match kind {
                SendErrorKind::Offline => {
                    return SendResult::Offline;
                }
                SendErrorKind::Timeout => {
                    return SendResult::Timeout;
                }
            },
            _ => {}
        },
    };
    let error_msg = String::from_utf8_lossy(&response_bytes).into_owned();
    return SendResult::DeserializationError(error_msg);
}

pub async fn send_parallel_requests<R>(
    targets: Vec<Address>,
    messages: Vec<impl serde::Serialize>,
    timeout_secs: u64,
) -> Vec<SendResult<R>>
where
    R: serde::de::DeserializeOwned,
{
    let mut futures = Vec::new();

    for (target, message) in targets.into_iter().zip(messages) {
        futures.push(send::<R>(message, target, timeout_secs));
    }

    let mut results = Vec::new();

    for future in futures {
        let result = future.await;
        results.push(result);
    }

    results
}

#[macro_export]
macro_rules! hyper {
    ($($code:tt)*) => {
        $crate::EXECUTOR.with(|ex| {
            ex.borrow_mut().spawn(async move {
                $($code)*
            })
        })
    };
}

// Enum defining the state persistance behaviour
#[derive(Clone)]
pub enum SaveOptions {
    // Never Persist State
    Never,
    // Persist State Every Message
    EveryMessage,
    // Persist State Every N Messages
    EveryNMessage(u64),
    // Persist State Every N Seconds
    EveryNSeconds(u64),
}
pub struct HiddenState {
    save_config: SaveOptions,
    message_count: u64,
}

impl HiddenState {
    pub fn new(save_config: SaveOptions) -> Self {
        Self {
            save_config,
            message_count: 0,
        }
    }

    fn should_save_state(&mut self) -> bool {
        match self.save_config {
            SaveOptions::Never => false,
            SaveOptions::EveryMessage => true,
            SaveOptions::EveryNMessage(n) => {
                self.message_count += 1;
                if self.message_count >= n {
                    self.message_count = 0;
                    true
                } else {
                    false
                }
            }
            SaveOptions::EveryNSeconds(_) => false, // Handled by timer instead
        }
    }
}

// TODO: We need a timer macro again.

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
    ui_config: Option<&hyperware_process_lib::http::server::HttpBindingConfig>,
    endpoints: &[Binding],
) -> http::server::HttpServer {
    let mut server = http::server::HttpServer::new(5);

    if let Some(ui) = ui_config {
        if let Err(e) = server.serve_ui("ui", vec!["/"], ui.clone()) {
            panic!("failed to serve UI: {e}. Make sure that a ui folder is in /pkg");
        }
    }

    // Verify no duplicate paths
    let mut seen_paths = std::collections::HashSet::new();
    for endpoint in endpoints.iter() {
        let path = match endpoint {
            Binding::Http { path, .. } => path,
            Binding::Ws { path, .. } => path,
        };
        if !seen_paths.insert(path) {
            panic!("duplicate path found: {}", path);
        }
    }

    for endpoint in endpoints {
        match endpoint {
            Binding::Http { path, config } => {
                server
                    .bind_http_path(path.to_string(), config.clone())
                    .expect("failed to serve API path");
            }
            Binding::Ws { path, config } => {
                server
                    .bind_ws_path(path.to_string(), config.clone())
                    .expect("failed to bind WS path");
            }
        }
    }

    server
}

fn handle_message<S, T1, T2, T3>(
    message: Message,
    user_state: &mut S,
    server: &mut http::server::HttpServer,
    handle_api_call: &impl Fn(&mut S, &str, T1),
    handle_local_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T2),
    handle_remote_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T3),
    handle_ws: &impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
) where
    S: State + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    T1: serde::Serialize + serde::de::DeserializeOwned,
    T2: serde::Serialize + serde::de::DeserializeOwned,
    T3: serde::Serialize + serde::de::DeserializeOwned,
{
    match message {
        Message::Response { body, context, .. } => {
            let correlation_id = context
                .as_deref()
                .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                .unwrap_or_else(|| "no context".to_string());

            RESPONSE_REGISTRY.with(|registry| {
                registry.borrow_mut().insert(correlation_id, body);
            });
        }
        Message::Request { .. } => {
            if message.is_local() {
                if message.source().process == "http-server:distro:sys" {
                    http_request(&message, user_state, server, handle_api_call, handle_ws);
                } else {
                    local_request(&message, user_state, server, handle_local_request);
                }
            } else {
                remote_request(&message, user_state, server, handle_remote_request);
            }
            // Try to save state based on configuration
            maybe_save_state(user_state);
        }
    }
}

pub fn app<S, T1, T2, T3>(
    app_name: &str,
    app_icon: Option<&str>,
    app_widget: Option<&str>,
    ui_config: Option<hyperware_process_lib::http::server::HttpBindingConfig>,
    endpoints: Vec<Binding>,
    save_config: SaveOptions,
    handle_api_call: impl Fn(&mut S, &str, T1),
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
    HIDDEN_STATE.with(|cell| {
        let mut hs = cell.borrow_mut();
        *hs = Some(HiddenState::new(save_config.clone()));
    });

    if app_icon.is_some() && app_widget.is_some() {
        homepage::add_to_homepage(app_name, app_icon, Some("/"), app_widget);
    }

    // Set up timer if needed
    if let SaveOptions::EveryNSeconds(_seconds) = save_config {
        // TODO: Needs to be asyncified
        // setup_periodic_save_timer::<S>(seconds);
    }

    move || {
        init_logging(Level::DEBUG, Level::INFO, None, Some((0, 0, 1, 1)), None).unwrap();
        info!("starting app");

        let mut server = setup_server(ui_config.as_ref(), &endpoints);
        let mut user_state = initialize_state::<S>();

        {
            init_fn(&mut user_state);
        }

        loop {
            EXECUTOR.with(|ex| ex.borrow_mut().poll_all_tasks());
            match await_message() {
                Err(send_error) => {
                    // TODO: We should either remove this or extend the async stuff here
                    handle_send_error::<S>(&send_error, &mut user_state);
                }
                Ok(message) => {
                    handle_message(
                        message,
                        &mut user_state,
                        &mut server,
                        &handle_api_call,
                        &handle_local_request,
                        &handle_remote_request,
                        &handle_ws,
                    );
                }
            }
        }
    }
}

fn http_request<S, T1>(
    message: &Message,
    state: &mut S,
    server: &mut http::server::HttpServer,
    handle_api_call: impl Fn(&mut S, &str, T1),
    handle_ws: impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
) where
    T1: serde::Serialize + serde::de::DeserializeOwned,
{
    let http_request = serde_json::from_slice::<http::server::HttpServerRequest>(message.body())
        .expect("failed to parse HTTP request");

    match http_request {
        HttpServerRequest::Http(http_request) => {
            let Ok(path) = http_request.path() else {
                warn!("Failed to get path for Http, exiting, this should never happen");
                send_response(StatusCode::BAD_REQUEST, None, vec![]);
                return;
            };
            let Some(blob) = message.blob() else {
                warn!("Failed to get blob for Http, exiting");
                send_response(StatusCode::BAD_REQUEST, None, vec![]);
                return;
            };
            let Ok(deserialized_struct) = serde_json::from_slice::<T1>(blob.bytes()) else {
                let body_str = String::from_utf8_lossy(blob.bytes());
                warn!("Raw request body was: {:#?}", body_str);
                warn!("Failed to deserialize into type parameter T1 (type: {}), check that the request body matches this type's structure", std::any::type_name::<T1>());
                send_response(StatusCode::BAD_REQUEST, None, vec![]);
                return;
            };

            handle_api_call(state, &path, deserialized_struct);
        }
        HttpServerRequest::WebSocketPush {
            channel_id,
            message_type,
        } => {
            let Some(blob) = message.blob() else {
                warn!("Failed to get blob for WebSocketPush, exiting");
                return;
            };
            handle_ws(state, server, channel_id, message_type, blob)
        }
        HttpServerRequest::WebSocketOpen { path, channel_id } => {
            server.handle_websocket_open(&path, channel_id);
        }
        HttpServerRequest::WebSocketClose(channel_id) => {
            server.handle_websocket_close(channel_id);
        }
    }
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
        } else {
            warn!("Failed to deserialize local request into struct, exiting");
            let body_str = String::from_utf8_lossy(message.body());
            warn!("Raw request body was: {:#?}", body_str);
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
        warn!("Failed to deserialize remote request into struct, exiting");
        let body_str = String::from_utf8_lossy(message.body());
        warn!("Raw request body was: {:#?}", body_str);
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

fn handle_send_error<S: Any + serde::Serialize>(send_error: &SendError, _user_state: &mut S) {
    // Print the error
    pretty_print_send_error(send_error);

    // If this is a timeout and we have a context (correlation_id), resolve the future with None
    if let SendError {
        kind,
        context: Some(context),
        ..
    } = send_error
    {
        // Convert context bytes to correlation_id string
        if let Ok(correlation_id) = String::from_utf8(context.to_vec()) {
            // Serialize None as the response
            let none_response = serde_json::to_vec(kind).unwrap();

            RESPONSE_REGISTRY.with(|registry| {
                registry.borrow_mut().insert(correlation_id, none_response);
            });
        }
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __check_not_all_empty {
    (_, _, _, _, _) => {
        compile_error!("At least one handler must be defined. Cannot use '_' for all handlers.");
    };
    ($($any:tt)*) => {};
}

// TODO: Rewrite the erect macro to use the new app function.
#[macro_export]
macro_rules! hyperprocess {
    (
        name: $name:expr,
        icon: $icon:expr,
        widget: $widget:expr,
        ui: $ui:expr,
        endpoints: [ $($endpoints:expr),* $(,)? ],
        save_config: $save_config:expr,
        handlers: {
            http: $http:tt,
            local: $local:tt,
            remote: $remote:tt,
            ws: $ws:tt,
        },
        init: $init:tt,
        wit_world: $wit_world:expr
        $(, additional_derives: [ $($additional_derives:path),* $(,)? ] )?
        $(,)?
    ) => {
        wit_bindgen::generate!({
            path: "target/wit",
            world: $wit_world,
            generate_unused_types: true,
            additional_derives: [
                serde::Deserialize,
                serde::Serialize,
                process_macros::SerdeJsonInto,
                $($($additional_derives,)*)?
            ],
        });

        $crate::__check_not_all_empty!($http, $local, $remote, $ws, $init);

        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                use hyperware_app_common::prelude::*;

                // Map `_` to the appropriate fallback function
                let handle_http_api_call = $crate::__maybe!($http => $crate::no_http_api_call);
                let handle_local_request = $crate::__maybe!($local => $crate::no_local_request);
                let handle_remote_request = $crate::__maybe!($remote => $crate::no_remote_request);
                let handle_ws = $crate::__maybe!($ws => $crate::no_ws_handler);
                let init_fn = $crate::__maybe!($init => $crate::no_init_fn);

                // Build the vector of endpoints from user input
                let endpoints_vec = vec![$($endpoints),*];

                let closure = $crate::app(
                    $name,
                    $icon,
                    $widget,
                    $ui,
                    endpoints_vec,
                    $save_config,
                    handle_http_api_call,
                    handle_local_request,
                    handle_remote_request,
                    handle_ws,
                    init_fn,
                );
                closure();
            }
        }
        export!(Component);
    };
}

// Re-export paste for use in our macros
pub use paste;

#[macro_export]
macro_rules! __maybe {
    // If the user wrote `_`, then expand to the default expression
    ( _ => $default:expr ) => {
        $default
    };
    // Otherwise use whatever they passed in
    ( $actual:expr => $default:expr ) => {
        $actual
    };
}

// For demonstration, we'll define them all in one place.
// Make sure the signatures match the real function signatures you require!
pub fn no_init_fn<S>(_state: &mut S) {
    // does nothing
}

pub fn no_ws_handler<S>(
    _state: &mut S,
    _server: &mut http::server::HttpServer,
    _channel_id: u32,
    _msg_type: http::server::WsMessageType,
    _blob: LazyLoadBlob,
) {
    // does nothing
}

pub fn no_http_api_call<S>(_state: &mut S, _path: &str, _req: ()) {
    // does nothing
}

pub fn no_local_request<S>(
    _msg: &Message,
    _state: &mut S,
    _server: &mut http::server::HttpServer,
    _req: (),
) {
    // does nothing
}

pub fn no_remote_request<S>(
    _msg: &Message,
    _state: &mut S,
    _server: &mut http::server::HttpServer,
    _req: (),
) {
    // does nothing
}

#[derive(Clone, Debug)]
pub enum Binding {
    Http {
        path: &'static str,
        config: hyperware_process_lib::http::server::HttpBindingConfig,
    },
    Ws {
        path: &'static str,
        config: hyperware_process_lib::http::server::WsBindingConfig,
    },
}

fn maybe_save_state<S>(state: &S)
where
    S: serde::Serialize,
{
    HIDDEN_STATE.with(|cell| {
        let mut hs = cell.borrow_mut();
        if let Some(ref mut hidden_state) = *hs {
            if hidden_state.should_save_state() {
                if let Ok(s_bytes) = rmp_serde::to_vec(state) {
                    kiprintln!("State persisted");
                    let _ = set_state(&s_bytes);
                }
            }
        }
    });
}
