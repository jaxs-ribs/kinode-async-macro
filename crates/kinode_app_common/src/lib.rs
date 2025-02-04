use kinode_process_lib::get_state;
use kinode_process_lib::http::server::WsMessageType;
use kinode_process_lib::logging::info;
use kinode_process_lib::logging::init_logging;
use kinode_process_lib::logging::Level;
use std::any::Any;
use std::collections::HashMap;

use std::cell::RefCell;

use kinode_process_lib::{
    await_message, homepage, http, kiprintln, set_state, LazyLoadBlob, Message, SendError,
};

pub mod prelude {
    pub use crate::HIDDEN_STATE;
    // Add other commonly used items here
}

thread_local! {
    pub static HIDDEN_STATE: RefCell<Option<HiddenState>> = RefCell::new(None);
}

/// The application state containing the callback map plus the user state. This is the actual state.
pub struct HiddenState {
    pub pending_callbacks: HashMap<String, PendingCallback>,
    pub accumulators: HashMap<String, Box<dyn Any + Send>>,
}

pub struct PendingCallback {
    pub on_success: Box<dyn FnOnce(&[u8], &mut dyn Any) -> anyhow::Result<()> + Send>,
    pub on_timeout: Option<Box<dyn FnOnce(&mut dyn Any) -> anyhow::Result<()> + Send>>,
}

pub struct FanOutAggregator<T> {
    total: usize,
    results: Vec<Option<Result<T, anyhow::Error>>>,
    completed: usize,

    /// Called once, when completed == total
    on_done:
        Box<dyn FnOnce(Vec<Result<T, anyhow::Error>>, &mut dyn Any) -> anyhow::Result<()> + Send>,
}

impl<T> FanOutAggregator<T> {
    pub fn new(
        total: usize,
        on_done: Box<
            dyn FnOnce(Vec<Result<T, anyhow::Error>>, &mut dyn Any) -> anyhow::Result<()> + Send,
        >,
    ) -> Self {
        let mut results = Vec::with_capacity(total);
        for _ in 0..total {
            results.push(None);
        }

        Self {
            total,
            results,
            completed: 0,
            on_done,
        }
    }

    pub fn set_result(&mut self, i: usize, result: Result<T, anyhow::Error>) {
        if i < self.results.len() && self.results[i].is_none() {
            self.results[i] = Some(result);
            self.completed += 1;
        }
    }

    pub fn is_done(&self) -> bool {
        self.completed >= self.total
    }

    /// Finalize => calls on_done(...) with the final vector
    pub fn finalize(self, user_state: &mut dyn Any) -> anyhow::Result<()> {
        let FanOutAggregator {
            results, on_done, ..
        } = self;
        let final_vec = results
            .into_iter()
            .map(|item| item.unwrap_or_else(|| Err(anyhow::anyhow!("missing subresponse"))))
            .collect();

        (on_done)(final_vec, user_state)
    }
}

impl HiddenState {
    pub fn new() -> Self {
        Self {
            pending_callbacks: HashMap::new(),
            accumulators: HashMap::new(),
        }
    }
}

#[macro_export]
macro_rules! timer {
    ($duration:expr, ($st:ident : $user_state_ty:ty) $callback_block:block) => {{
        use uuid::Uuid;

        let correlation_id = Uuid::new_v4().to_string();

        $crate::HIDDEN_STATE.with(|cell| {
            let mut hs = cell.borrow_mut();
            if let Some(ref mut hidden_state) = *hs {
                hidden_state.pending_callbacks.insert(
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
        });

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

fn handle_message<S, T1, T2, T3>(
    message: Message,
    user_state: &mut S,
    server: &mut http::server::HttpServer,
    handle_api_call: &impl Fn(&mut S, T1) -> (http::server::HttpResponse, Vec<u8>),
    handle_local_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T2),
    handle_remote_request: &impl Fn(&Message, &mut S, &mut http::server::HttpServer, T3),
    handle_ws: &impl Fn(&mut S, &mut http::server::HttpServer, u32, WsMessageType, LazyLoadBlob),
) where
    S: State + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    T1: serde::Serialize + serde::de::DeserializeOwned,
    T2: serde::Serialize + serde::de::DeserializeOwned,
    T3: serde::Serialize + serde::de::DeserializeOwned,
{
    // Get the correlation id
    let correlation_id = message
        .context()
        .map(|c| String::from_utf8_lossy(c).to_string());

    if let Some(cid) = correlation_id {
        // --------------------------------------------------------------------------------------------
        // Regular callback case
        // --------------------------------------------------------------------------------------------
        let pending = HIDDEN_STATE.with(|cell| {
            let mut hs = cell.borrow_mut();
            hs.as_mut()
                .and_then(|state| state.pending_callbacks.remove(&cid))
        });
        if let Some(pending) = pending {
            // EXECUTE the callback
            if let Err(e) = (pending.on_success)(message.body(), user_state) {
                kiprintln!("Error in callback: {e}");
            }
            // Get a fresh borrow of user_state for serializing:
            if let Ok(s_bytes) = rmp_serde::to_vec(user_state) {
                let _ = set_state(&s_bytes);
            }
            return;
        }

        // --------------------------------------------------------------------------------------------
        // Fan out case
        // --------------------------------------------------------------------------------------------
        if let Some((agg_id, idx)) = parse_aggregator_cid(&cid) {
            let result = match serde_json::from_slice::<serde_json::Value>(message.body()) {
                Ok(val) => Ok(val),
                Err(e) => Err(anyhow::anyhow!(e)),
            };

            aggregator_mark_result(&agg_id, idx, result, user_state);
            return;
        }
    }

    if message.is_local() {
        if message.source().process == "http-server:distro:sys" {
            http_request(&message, user_state, server, handle_api_call, handle_ws);
        } else {
            local_request(&message, user_state, server, handle_local_request);
        }
    } else {
        remote_request(&message, user_state, server, handle_remote_request);
    }

    // Persist the state with a new temporary borrow.
    if let Ok(s_bytes) = rmp_serde::to_vec(user_state) {
        let _ = set_state(&s_bytes);
    }
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
    HIDDEN_STATE.with(|cell| {
        let mut hs = cell.borrow_mut();
        *hs = Some(HiddenState::new());
    });

    if app_icon.is_some() && app_widget.is_some() {
        homepage::add_to_homepage(app_name, app_icon, Some("/"), app_widget);
    }

    move || {
        init_logging(Level::DEBUG, Level::INFO, None, Some((0, 0, 1, 1)), None).unwrap();
        info!("starting app");

        let mut server = setup_server(&ui_config, &api_config, &ws_config);
        let mut user_state = initialize_state::<S>();

        // Execute the user specified init function
        // Note: It should always be called _after_ the hidden state is initialized, so that
        // users can call macros that depend on having the callback map in the hidden state.
        {
            init_fn(&mut user_state);
        }

        loop {
            match await_message() {
                Err(send_error) => {
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

fn handle_send_error<S: Any + serde::Serialize>(send_error: &SendError, user_state: &mut S) {
    // Print the error
    pretty_print_send_error(send_error);

    // Get the correlation id
    let Some(correlation_id) = send_error
        .context
        .as_ref()
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
    else {
        return;
    };

    // --------------------------------------------------------------------------------------------
    // Handle the single callback case
    // --------------------------------------------------------------------------------------------
    let single_callback = HIDDEN_STATE.with(|cell| {
        let mut hs = cell.borrow_mut();
        hs.as_mut()
            .and_then(|st| st.pending_callbacks.remove(&correlation_id))
    });
    if let Some(single_callback) = single_callback {
        if let Some(cb) = single_callback.on_timeout {
            if let Err(e) = cb(user_state) {
                kiprintln!("Error in on_timeout callback: {e}");
            }
            // Persist the state
            if let Ok(s_bytes) = rmp_serde::to_vec(&user_state) {
                let _ = set_state(&s_bytes);
            }
        }
    }

    // --------------------------------------------------------------------------------------------
    // Handle the fan out case
    // --------------------------------------------------------------------------------------------
    if let Some((agg_id, idx)) = parse_aggregator_cid(&correlation_id) {
        aggregator_mark_result(&agg_id, idx, Err(anyhow::anyhow!("timeout")), user_state);

        if let Ok(s_bytes) = rmp_serde::to_vec(&user_state) {
            let _ = set_state(&s_bytes);
        }
    }
}

// Parse a correlation id of the form "agg_id:idx"
fn parse_aggregator_cid(corr: &str) -> Option<(String, usize)> {
    let mut parts = corr.split(':');
    let agg_id = parts.next()?.to_owned();
    let idx_str = parts.next()?;
    let idx = idx_str.parse::<usize>().ok()?;
    Some((agg_id, idx))
}

#[doc(hidden)]
#[macro_export]
macro_rules! __check_not_all_empty {
    (_, _, _, _, _) => {
        compile_error!("At least one handler must be defined. Cannot use '_' for all handlers.");
    };
    ($($any:tt)*) => {};
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
        $handle_api_call:tt,
        $handle_local_request:tt,
        $handle_remote_request:tt,
        $handle_ws:tt,
        $init_fn:tt
    ) => {
        // First check that not all handlers are empty
        $crate::__check_not_all_empty!($handle_api_call, $handle_local_request, $handle_remote_request, $handle_ws, $init_fn);

        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                use kinode_app_common::prelude::*;

                let init_closure = $crate::app(
                    $app_name,
                    $app_icon,
                    $app_widget,
                    $ui_config,
                    $api_config,
                    $ws_config,
                    $crate::__maybe!($handle_api_call => $crate::no_http_api_call),
                    $crate::__maybe!($handle_local_request => $crate::no_local_request),
                    $crate::__maybe!($handle_remote_request => $crate::no_remote_request),
                    $crate::__maybe!($handle_ws => $crate::no_ws_handler),
                    $crate::__maybe!($init_fn => $crate::no_init_fn),
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

pub fn aggregator_mark_result(
    aggregator_id: &str,
    i: usize,
    result: anyhow::Result<serde_json::Value>,
    user_state_any: &mut dyn Any,
) { // Indentationmaxxing
    HIDDEN_STATE.with(|cell| {
        if let Some(ref mut hidden) = *cell.borrow_mut() {
            if let Some(acc_any) = hidden.accumulators.get_mut(aggregator_id) {
                if let Some(agg) = acc_any.downcast_mut::<FanOutAggregator<serde_json::Value>>() {
                    agg.set_result(i, result);
                    if agg.is_done() {
                        let aggregator_box = hidden.accumulators.remove(aggregator_id).unwrap();
                        if let Ok(agg2) =
                            aggregator_box.downcast::<FanOutAggregator<serde_json::Value>>()
                        {
                            let aggregator = *agg2;
                            if let Err(e) = aggregator.finalize(user_state_any) {
                                kiprintln!("Error finalizing aggregator: {e}");
                            }
                        }
                    }
                }
            }
        }
    });
}

#[macro_export]
macro_rules! fan_out {
    (
        $addresses:expr,
        $requests:expr,
        ($all_results:ident, $st:ident : $st_ty:ty) $done_block:block,
        $timeout:expr
    ) => {{
        use ::anyhow::{anyhow, Result as AnyResult};
        use ::uuid::Uuid;
        use ::serde_json;

        // A unique aggregator ID for this fan-out sequence
        let aggregator_id = Uuid::new_v4().to_string();
        let total = $addresses.len();

        // Build the aggregator that will hold partial results and run your final callback
        let aggregator = $crate::FanOutAggregator::new(
            total,
            Box::new(move |results: Vec<AnyResult<serde_json::Value>>, user_state: &mut dyn std::any::Any| -> AnyResult<()> {
                let $st = user_state
                    .downcast_mut::<$st_ty>()
                    .ok_or_else(|| anyhow!("Downcast failed!"))?;

                let $all_results = results;
                $done_block
                Ok(())
            }),
        );

        // Insert this aggregator into HIDDEN_STATE's accumulators
        $crate::HIDDEN_STATE.with(|cell| {
            if let Some(ref mut hidden_state) = *cell.borrow_mut() {
                hidden_state
                    .accumulators
                    .insert(aggregator_id.clone(), Box::new(aggregator));
            }
        });

        // For each address+request, serialize/send, and if there's an immediate error, mark aggregator
        for (i, address) in $addresses.iter().enumerate() {
            let correlation_id = format!("{}:{}", aggregator_id, i);

            // Serialize request
            let body = match serde_json::to_vec(&$requests[i]) {
                Ok(b) => b,
                Err(e) => {
                    // Mark aggregator's i'th result as an error
                    $crate::HIDDEN_STATE.with(|cell| {
                        if let Some(ref mut hidden_st) = *cell.borrow_mut() {
                            $crate::aggregator_mark_result(
                                &aggregator_id,
                                i,
                                Err(anyhow!("Error serializing request: {}", e)),
                                hidden_st as &mut dyn std::any::Any,
                            );
                        }
                    });
                    continue;
                }
            };

            // Send the request - using fully qualified path
            let send_result = kinode_process_lib::Request::to(address)
                .body(body)
                .expects_response($timeout)
                .context(correlation_id.as_bytes())
                .send();

            // If the immediate send fails, also mark aggregator
            if let Err(e) = send_result {
                $crate::HIDDEN_STATE.with(|cell| {
                    if let Some(ref mut hidden_st) = *cell.borrow_mut() {
                        $crate::aggregator_mark_result(
                            &aggregator_id,
                            i,
                            Err(anyhow!("Send error: {:?}", e)),
                            hidden_st as &mut dyn std::any::Any,
                        );
                    }
                });
            }
        }
    }};
}

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

pub fn no_http_api_call<S>(
    _state: &mut S,
    _req: (),
) -> (http::server::HttpResponse, Vec<u8>) {
    // trivial 200
    (http::server::HttpResponse::new(200 as u16), vec![])
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