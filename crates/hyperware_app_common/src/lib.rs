#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
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
use hyperware_process_lib::http::server::HttpServer;
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
    pub use crate::APP_CONTEXT;
    // Add other commonly used items here
}

thread_local! {
    pub static APP_CONTEXT: RefCell<AppContext> = RefCell::new(AppContext {
        hidden_state: None,
        executor: Executor::new(),
        response_registry: HashMap::new(),
        current_path: None,
        current_server: None,
    });
}

pub struct AppContext {
    pub hidden_state: Option<HiddenState>,
    pub executor: Executor,
    pub response_registry: HashMap<String, Vec<u8>>,
    pub current_path: Option<String>,
    pub current_server: Option<*mut HttpServer>,
}

// Access function for the current path
pub fn get_path() -> Option<String> {
    APP_CONTEXT.with(|ctx| ctx.borrow().current_path.clone())
}

// Access function for the current server
pub fn get_server() -> Option<&'static mut HttpServer> {
    APP_CONTEXT.with(|ctx| ctx.borrow().current_server.map(|ptr| unsafe { &mut *ptr }))
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

        let maybe_bytes = APP_CONTEXT.with(|ctx| {
            let mut ctx_mut = ctx.borrow_mut();
            ctx_mut.response_registry.remove(correlation_id)
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
        $crate::APP_CONTEXT.with(|ctx| {
            ctx.borrow_mut().executor.spawn(async move {
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
pub fn initialize_state<S>() -> S
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

pub fn setup_server(
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

/// Pretty prints a SendError in a more readable format
pub fn pretty_print_send_error(error: &SendError) {
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

pub fn handle_send_error<S: Any + serde::Serialize>(send_error: &SendError, _user_state: &mut S) {
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

            APP_CONTEXT.with(|ctx| {
                ctx.borrow_mut().response_registry.insert(correlation_id, none_response);
            });
        }
    }
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

pub fn no_http_api_call<S>(_state: &mut S, _req: ()) {
    // does nothing
}

pub fn no_local_request<S>(
    _msg: &Message,
    _state: &mut S,
    _req: (),
) {
    // does nothing
}

pub fn no_remote_request<S>(
    _msg: &Message,
    _state: &mut S,
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

pub fn maybe_save_state<S>(state: &S)
where
    S: serde::Serialize,
{
    APP_CONTEXT.with(|ctx| {
        let mut ctx_mut = ctx.borrow_mut();
        if let Some(ref mut hidden_state) = ctx_mut.hidden_state {
            if hidden_state.should_save_state() {
                if let Ok(s_bytes) = rmp_serde::to_vec(state) {
                    kiprintln!("State persisted");
                    let _ = set_state(&s_bytes);
                }
            }
        }
    });
}
