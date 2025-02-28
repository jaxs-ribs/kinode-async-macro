use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_macro_input, spanned::Spanned, Expr, ItemImpl, Meta};

// Create a newtype wrapper
struct MetaList(Punctuated<Meta, Comma>);

struct HyperProcessArgs {
    name: String,
    icon: Option<String>,
    widget: Option<String>,
    ui: Option<Expr>,
    endpoints: Expr,
    save_config: Expr,
    wit_world: String,
}

mod kw {
    syn::custom_keyword!(name);
    syn::custom_keyword!(icon);
    syn::custom_keyword!(widget);
    syn::custom_keyword!(ui);
    syn::custom_keyword!(endpoints);
    syn::custom_keyword!(save_config);
    syn::custom_keyword!(wit_world);
}

// Implement Parse for our newtype wrapper instead
impl syn::parse::Parse for MetaList {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut args = Punctuated::new();
        while !input.is_empty() {
            args.push_value(input.parse()?);
            if input.is_empty() {
                break;
            }
            args.push_punct(input.parse()?);
        }
        Ok(MetaList(args))
    }
}

fn parse_args(attr_args: MetaList) -> syn::Result<HyperProcessArgs> {
    let mut name = None;
    let mut icon = None;
    let mut widget = None;
    let mut ui = None;
    let mut endpoints = None;
    let mut save_config = None;
    let mut wit_world = None;

    let span = attr_args
        .0
        .first()
        .map_or_else(|| proc_macro2::Span::call_site(), |arg| arg.span());

    for arg in &attr_args.0 {
        if let Meta::NameValue(nv) = arg {
            let key = nv.path.get_ident().unwrap().to_string();
            match key.as_str() {
                "name" => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit) = &expr_lit.lit {
                            name = Some(lit.value());
                        } else {
                            return Err(syn::Error::new(
                                nv.value.span(),
                                "Expected string literal",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(nv.value.span(), "Expected string literal"));
                    }
                }
                "icon" => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit) = &expr_lit.lit {
                            icon = Some(lit.value());
                        } else {
                            return Err(syn::Error::new(
                                nv.value.span(),
                                "Expected string literal",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(nv.value.span(), "Expected string literal"));
                    }
                }
                "widget" => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit) = &expr_lit.lit {
                            widget = Some(lit.value());
                        } else {
                            return Err(syn::Error::new(
                                nv.value.span(),
                                "Expected string literal",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(nv.value.span(), "Expected string literal"));
                    }
                }
                "ui" => {
                    if let syn::Expr::Call(call) = &nv.value {
                        if let syn::Expr::Path(path) = &*call.func {
                            if path
                                .path
                                .segments
                                .last()
                                .map(|s| s.ident == "Some")
                                .unwrap_or(false)
                            {
                                if call.args.len() == 1 {
                                    ui = Some(call.args[0].clone());
                                } else {
                                    return Err(syn::Error::new(
                                        call.span(),
                                        "Some must have exactly one argument",
                                    ));
                                }
                            }
                        }
                    } else {
                        ui = Some(nv.value.clone());
                    }
                }
                "endpoints" => endpoints = Some(nv.value.clone()),
                "save_config" => save_config = Some(nv.value.clone()),
                "wit_world" => {
                    if let syn::Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit) = &expr_lit.lit {
                            wit_world = Some(lit.value());
                        } else {
                            return Err(syn::Error::new(
                                nv.value.span(),
                                "Expected string literal",
                            ));
                        }
                    } else {
                        return Err(syn::Error::new(nv.value.span(), "Expected string literal"));
                    }
                }
                _ => return Err(syn::Error::new(nv.path.span(), "Unknown attribute")),
            }
        } else {
            return Err(syn::Error::new(arg.span(), "Expected name-value pair"));
        }
    }

    Ok(HyperProcessArgs {
        name: name.ok_or_else(|| syn::Error::new(span, "Missing 'name'"))?,
        icon,
        widget,
        ui,
        endpoints: endpoints.ok_or_else(|| syn::Error::new(span, "Missing 'endpoints'"))?,
        save_config: save_config.ok_or_else(|| syn::Error::new(span, "Missing 'save_config'"))?,
        wit_world: wit_world.ok_or_else(|| syn::Error::new(span, "Missing 'wit_world'"))?,
    })
}

struct MethodSignatureSpec {
    param_count: usize,
    param_types: Vec<(&'static str, &'static str)>, // (expected_type, error_message)
    handler_name: &'static str,
}

fn validate_method_signature(
    method: &syn::ImplItemFn,
    spec: MethodSignatureSpec,
) -> syn::Result<()> {
    // Validate parameter count
    if method.sig.inputs.len() != spec.param_count {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "{} handler must take {} parameters",
                spec.handler_name, spec.param_count
            ),
        ));
    }

    // Skip first parameter (assumed to be &mut self)
    for (idx, (expected_type, error_msg)) in spec.param_types.iter().enumerate() {
        match &method.sig.inputs[idx + 1] {
            syn::FnArg::Typed(pat)
                if pat.ty.as_ref().to_token_stream().to_string() == *expected_type => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    &method.sig.inputs[idx + 1],
                    error_msg,
                ))
            }
        }
    }

    // Validate return type
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!("{} handler must not return a value", spec.handler_name),
        ));
    }

    Ok(())
}

fn validate_init_method(method: &syn::ImplItemFn) -> syn::Result<()> {
    let spec = MethodSignatureSpec {
        param_count: 1,
        param_types: vec![],
        handler_name: "Init",
    };

    // Special case for init method since it only needs &mut self
    if !matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Init method must take only &mut self",
        ));
    }

    validate_method_signature(method, spec)
}

fn validate_http_method(method: &syn::ImplItemFn) -> syn::Result<()> {
    validate_message_handler(method, "HTTP")
}

fn validate_message_handler(method: &syn::ImplItemFn, handler_name: &str) -> syn::Result<()> {
    if method.sig.inputs.len() != 3 {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "{} handler must take exactly three parameters: &mut self, &Message, and req",
                handler_name
            ),
        ));
    }
    // First parameter must be &mut self
    if !matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "First parameter must be &mut self",
        ));
    }
    // Second parameter must be &Message
    if let syn::FnArg::Typed(pat) = &method.sig.inputs[1] {
        if pat.ty.to_token_stream().to_string() != "& Message" {
            return Err(syn::Error::new_spanned(
                &method.sig.inputs[1],
                "Second parameter must be &Message",
            ));
        }
    } else {
        return Err(syn::Error::new_spanned(
            &method.sig.inputs[1],
            "Second parameter must be a typed parameter",
        ));
    }
    // No return type
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!("{} handler must not return a value", handler_name),
        ));
    }
    Ok(())
}

fn validate_ws_method(method: &syn::ImplItemFn) -> syn::Result<()> {
    let spec = MethodSignatureSpec {
        param_count: 5,
        param_types: vec![
            (
                "& mut HttpServer",
                "Second parameter must be &mut HttpServer",
            ),
            ("u32", "Third parameter must be u32"),
            ("WsMessageType", "Fourth parameter must be WsMessageType"),
            ("LazyLoadBlob", "Fifth parameter must be LazyLoadBlob"),
        ],
        handler_name: "WS",
    };
    validate_method_signature(method, spec)
}

// Modified to return vectors of method identifiers instead of single options
fn analyze_methods(
    impl_block: &ItemImpl,
) -> syn::Result<(
    Option<syn::Ident>,
    Vec<syn::Ident>,
    Vec<syn::Ident>,
    Vec<syn::Ident>,
    Option<syn::Ident>,
)> {
    let mut init_method = None;
    let mut http_methods = Vec::new();
    let mut local_methods = Vec::new();
    let mut remote_methods = Vec::new();
    let mut ws_method = None;

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            let ident = method.sig.ident.clone();
            let mut has_init = false;
            let mut has_http = false;
            let mut has_local = false;
            let mut has_remote = false;
            let mut has_ws = false;

            // Collect attributes for this method
            for attr in &method.attrs {
                if attr.path().is_ident("init") {
                    has_init = true;
                } else if attr.path().is_ident("http") {
                    has_http = true;
                } else if attr.path().is_ident("local") {
                    has_local = true;
                } else if attr.path().is_ident("remote") {
                    has_remote = true;
                } else if attr.path().is_ident("ws") {
                    has_ws = true;
                }
            }

            // Validate init method (exclusive)
            if has_init {
                if has_http || has_local || has_remote || has_ws {
                    return Err(syn::Error::new_spanned(
                        method,
                        "#[init] cannot be combined with other attributes",
                    ));
                }

                validate_init_method(method)?;

                if init_method.is_some() {
                    return Err(syn::Error::new_spanned(
                        method,
                        "Multiple #[init] methods defined",
                    ));
                }

                init_method = Some(ident);
                continue;
            }

            // Validate ws method (exclusive)
            if has_ws {
                if has_http || has_local || has_remote || has_init {
                    return Err(syn::Error::new_spanned(
                        method,
                        "#[ws] cannot be combined with other attributes",
                    ));
                }

                validate_ws_method(method)?;

                if ws_method.is_some() {
                    return Err(syn::Error::new_spanned(
                        method,
                        "Multiple #[ws] methods defined",
                    ));
                }

                ws_method = Some(ident);
                continue;
            }

            // Add to appropriate vectors - a method can be in multiple vectors if it has multiple attributes
            // Validate http methods and add to the vector
            if has_http {
                validate_http_method(method)?;
                http_methods.push(ident.clone());
            }

            // Validate local methods and add to the vector
            if has_local {
                validate_message_handler(method, "local")?;
                local_methods.push(ident.clone());
            }

            // Validate remote methods and add to the vector
            if has_remote {
                validate_message_handler(method, "remote")?;
                remote_methods.push(ident.clone());
            }
        }
    }

    Ok((
        init_method,
        http_methods,
        local_methods,
        remote_methods,
        ws_method,
    ))
}

#[proc_macro_attribute]
pub fn hyperprocess(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(attr as MetaList);
    let impl_block = parse_macro_input!(item as ItemImpl);

    let args = match parse_args(attr_args) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    let self_ty = &impl_block.self_ty;

    let (init_method, http_methods, local_methods, remote_methods, ws_method) =
        match analyze_methods(&impl_block) {
            Ok(methods) => methods,
            Err(e) => return e.to_compile_error().into(),
        };

    let init_fn_code = if let Some(method_name) = init_method {
        quote! { |state: &mut #self_ty| state.#method_name() }
    } else {
        quote! { no_init_fn }
    };

    // Generate code for HTTP handlers that properly handles ownership
    let handle_http_code = if !http_methods.is_empty() {
        let method_calls = http_methods
            .iter()
            .enumerate()
            .map(|(i, method_name)| {
                if i < http_methods.len() - 1 {
                    // Clone for all but the last one
                    quote! { state.#method_name(&dummy_msg, req.clone()); }
                } else {
                    // Consume the original for the last one
                    quote! { state.#method_name(&dummy_msg, req); }
                }
            })
            .collect::<Vec<_>>();

        quote! {
            |state: &mut #self_ty, req: serde_json::Value| {
                // Create a bogus message to pass to the handler
                let dummy_msg = unsafe { std::mem::zeroed::<hyperware_process_lib::Message>() };
                #(#method_calls)*
            }
        }
    } else {
        quote! { no_http_api_call }
    };

    // Generate code for local message handlers
    let handle_local_code = if !local_methods.is_empty() {
        let method_calls = local_methods
            .iter()
            .enumerate()
            .map(|(i, method_name)| {
                if i < local_methods.len() - 1 {
                    // Clone for all but the last one
                    quote! { state.#method_name(message, req.clone()); }
                } else {
                    // Consume the original for the last one
                    quote! { state.#method_name(message, req); }
                }
            })
            .collect::<Vec<_>>();

        quote! {
            |message: &Message, state: &mut #self_ty, req: serde_json::Value| {
                #(#method_calls)*
            }
        }
    } else {
        quote! { no_local_request }
    };

    // Generate code for remote message handlers
    let handle_remote_code = if !remote_methods.is_empty() {
        let method_calls = remote_methods
            .iter()
            .enumerate()
            .map(|(i, method_name)| {
                if i < remote_methods.len() - 1 {
                    // Clone for all but the last one
                    quote! { state.#method_name(message, req.clone()); }
                } else {
                    // Consume the original for the last one
                    quote! { state.#method_name(message, req); }
                }
            })
            .collect::<Vec<_>>();

        quote! {
            |message: &Message, state: &mut #self_ty, req: serde_json::Value| {
                #(#method_calls)*
            }
        }
    } else {
        quote! { no_remote_request }
    };

    // WebSocket handler remains the same since only one is allowed
    let handle_ws_code = if let Some(method_name) = ws_method {
        quote! { |state: &mut #self_ty, server: &mut HttpServer, channel_id: u32, msg_type: WsMessageType, blob: LazyLoadBlob| state.#method_name(server, channel_id, msg_type, blob) }
    } else {
        quote! { no_ws_handler }
    };

    let icon = args
        .icon
        .as_ref()
        .map(|s| quote! { Some(#s) })
        .unwrap_or(quote! { None });
    let widget = args
        .widget
        .as_ref()
        .map(|s| quote! { Some(#s) })
        .unwrap_or(quote! { None });
    let ui = args
        .ui
        .as_ref()
        .map(|expr| quote! { Some(#expr) })
        .unwrap_or(quote! { None });

    let mut cleaned_impl_block = impl_block.clone();
    for item in &mut cleaned_impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr.path().is_ident("init")
                    && !attr.path().is_ident("http")
                    && !attr.path().is_ident("local")
                    && !attr.path().is_ident("remote")
                    && !attr.path().is_ident("ws")
            });
        }
    }

    let name = &args.name;
    let endpoints = &args.endpoints;
    let save_config = &args.save_config;
    let wit_world = &args.wit_world;

    let output = quote! {
        wit_bindgen::generate!({
            path: "target/wit",
            world: #wit_world,
            generate_unused_types: true,
            additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
        });

        #cleaned_impl_block

        // Create a module to isolate our imports and avoid name collisions
        mod __hyperprocess_internal {
            // Re-export the necessary items for use in the Component impl
            pub use hyperware_app_common::{
                no_init_fn,
                no_http_api_call,
                no_local_request,
                no_remote_request,
                no_ws_handler,
                Binding,
                SaveOptions,
            };
            pub use hyperware_process_lib::{
                http::server::{HttpServer, WsMessageType},
                LazyLoadBlob,
                Message,
            };
            // Import futures_util directly from the crate
            pub use futures_util::task::noop_waker_ref;

            // Import all the necessary items for the generated code
            use hyperware_app_common::prelude::*;
            use hyperware_app_common::{
                HiddenState,
                State,
                EXECUTOR,
                RESPONSE_REGISTRY,
                CURRENT_PATH,
                CURRENT_SERVER,
                maybe_save_state,
                handle_send_error,
                setup_server,
                initialize_state,
            };
            use hyperware_process_lib::get_state;
            use hyperware_process_lib::http::server::send_response;
            use hyperware_process_lib::http::server::HttpServerRequest;
            use hyperware_process_lib::http::StatusCode;
            use hyperware_process_lib::logging::info;
            use hyperware_process_lib::logging::init_logging;
            use hyperware_process_lib::logging::warn;
            use hyperware_process_lib::logging::Level;
            use hyperware_process_lib::Address;
            use hyperware_process_lib::Request;
            use hyperware_process_lib::SendErrorKind;
            use hyperware_process_lib::{
                await_message, homepage, http, kiprintln, set_state, SendError,
            };
            use serde::Deserialize;
            use serde::Serialize;
            use serde_json::Value;
            use std::any::Any;
            use std::cell::RefCell;
            use std::collections::HashMap;
            use std::future::Future;
            use std::pin::Pin;
            use std::task::{Context, Poll};
            use uuid::Uuid;

            // Define the app function inside the module
            pub fn app<S, T1, T2, T3>(
                app_name: &str,
                app_icon: Option<&str>,
                app_widget: Option<&str>,
                ui_config: Option<hyperware_process_lib::http::server::HttpBindingConfig>,
                endpoints: Vec<Binding>,
                save_config: SaveOptions,
                handle_api_call: impl Fn(&mut S, T1),
                handle_local_request: impl Fn(&Message, &mut S, T2),
                handle_remote_request: impl Fn(&Message, &mut S, T3),
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

            // Include all the helper functions inside the module
            fn handle_message<S, T1, T2, T3>(
                message: Message,
                user_state: &mut S,
                server: &mut http::server::HttpServer,
                handle_api_call: &impl Fn(&mut S, T1),
                handle_local_request: &impl Fn(&Message, &mut S, T2),
                handle_remote_request: &impl Fn(&Message, &mut S, T3),
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

            fn http_request<S, T1>(
                message: &Message,
                state: &mut S,
                server: &mut http::server::HttpServer,
                handle_api_call: impl Fn(&mut S, T1),
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

                        CURRENT_PATH.with(|cp| *cp.borrow_mut() = Some(path.clone()));
                        handle_api_call(state, deserialized_struct);
                        CURRENT_PATH.with(|cp| *cp.borrow_mut() = None);
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
                handle_local_request: &impl Fn(&Message, &mut S, T),
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
                CURRENT_SERVER.with(|cs| *cs.borrow_mut() = Some(server as *mut HttpServer));
                handle_local_request(message, state, request);
                CURRENT_SERVER.with(|cs| *cs.borrow_mut() = None);
            }

            fn remote_request<S, T>(
                message: &Message,
                state: &mut S,
                server: &mut http::server::HttpServer,
                handle_remote_request: &impl Fn(&Message, &mut S, T),
            ) where
                T: serde::Serialize + serde::de::DeserializeOwned,
            {
                let Ok(request) = serde_json::from_slice::<T>(message.body()) else {
                    warn!("Failed to deserialize remote request into struct, exiting");
                    let body_str = String::from_utf8_lossy(message.body());
                    warn!("Raw request body was: {:#?}", body_str);
                    return;
                };
                CURRENT_SERVER.with(|cs| *cs.borrow_mut() = Some(server as *mut HttpServer));
                handle_remote_request(message, state, request);
                CURRENT_SERVER.with(|cs| *cs.borrow_mut() = None);
            }
        }

        // Don't add extern crate here - use the proper path instead
        
        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                // Use the re-exported items from our internal module
                use __hyperprocess_internal::{
                    no_init_fn, no_http_api_call, no_local_request, no_remote_request, no_ws_handler,
                    Binding, Message, HttpServer, WsMessageType, LazyLoadBlob, SaveOptions, noop_waker_ref
                };

                let init_fn = #init_fn_code;
                let handle_http = #handle_http_code;
                let handle_local = #handle_local_code;
                let handle_remote = #handle_remote_code;
                let handle_ws = #handle_ws_code;

                let endpoints_vec = #endpoints;

                let closure = __hyperprocess_internal::app(
                    #name,
                    #icon,
                    #widget,
                    #ui,
                    endpoints_vec,
                    #save_config,
                    handle_http,
                    handle_local,
                    handle_remote,
                    handle_ws,
                    init_fn,
                );
                closure();
            }
        }

        export!(Component);
    };

    output.into()
}
