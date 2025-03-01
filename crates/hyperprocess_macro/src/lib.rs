#![allow(warnings)] // TODO: Zena: Remove this and fix warnings

//! # HyperProcess Procedural Macro
//! 
//! This macro generates boilerplate code for HyperProcess applications,
//! analyzing method attributes to create appropriate request/response handling.

use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse_macro_input, spanned::Spanned, 
    Expr, ItemImpl, Meta, ReturnType,
    punctuated::Punctuated, token::Comma,
};

//------------------------------------------------------------------------------
// Type Definitions
//------------------------------------------------------------------------------

/// Keywords for parsing attribute arguments
mod kw {
    syn::custom_keyword!(name);
    syn::custom_keyword!(icon);
    syn::custom_keyword!(widget);
    syn::custom_keyword!(ui);
    syn::custom_keyword!(endpoints);
    syn::custom_keyword!(save_config);
    syn::custom_keyword!(wit_world);
}

/// A wrapper for a punctuated list of Meta items
struct MetaList(Punctuated<Meta, Comma>);

/// Arguments for the hyperprocess macro
struct HyperProcessArgs {
    name: String,
    icon: Option<String>,
    widget: Option<String>,
    ui: Option<Expr>,
    endpoints: Expr,
    save_config: Expr,
    wit_world: String,
}

/// Metadata for a function in the implementation block
struct FunctionMetadata {
    name: syn::Ident,                // Original function name
    variant_name: String,            // CamelCase variant name
    params: Vec<syn::Type>,          // Parameter types (excluding &mut self)
    return_type: Option<syn::Type>,  // Return type (None for functions returning ())
    is_async: bool,                  // Whether function is async
    is_local: bool,                  // Has #[local] attribute
    is_remote: bool,                 // Has #[remote] attribute
    is_http: bool,                   // Has #[http] attribute
}

// Enum for the different handler types
enum HandlerType {
    Local,
    Remote,
    Http
}

//------------------------------------------------------------------------------
// Utility Functions
//------------------------------------------------------------------------------

/// Convert a snake_case string to CamelCase
fn to_camel_case(snake: &str) -> String {
    let mut camel = String::new();
    let mut capitalize_next = true;
    
    for c in snake.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            camel.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            camel.push(c);
        }
    }
    
    camel
}

/// Parse a string literal from an expression
fn parse_string_literal(expr: &Expr, span: proc_macro2::Span) -> syn::Result<String> {
    if let Expr::Lit(expr_lit) = expr {
        if let syn::Lit::Str(lit) = &expr_lit.lit {
            Ok(lit.value())
        } else {
            Err(syn::Error::new(span, "Expected string literal"))
        }
    } else {
        Err(syn::Error::new(span, "Expected string literal"))
    }
}

/// Check if a method has a specific attribute
fn has_attribute(method: &syn::ImplItemFn, attr_name: &str) -> bool {
    method.attrs.iter().any(|attr| attr.path().is_ident(attr_name))
}

//------------------------------------------------------------------------------
// Parsing Implementation
//------------------------------------------------------------------------------

/// Implement Parse for our MetaList newtype wrapper
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

/// Parse the arguments to the hyperprocess macro
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
                    name = Some(parse_string_literal(&nv.value, nv.value.span())?);
                }
                "icon" => {
                    icon = Some(parse_string_literal(&nv.value, nv.value.span())?);
                }
                "widget" => {
                    widget = Some(parse_string_literal(&nv.value, nv.value.span())?);
                }
                "ui" => {
                    if let Expr::Call(call) = &nv.value {
                        if let Expr::Path(path) = &*call.func {
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
                    wit_world = Some(parse_string_literal(&nv.value, nv.value.span())?);
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

//------------------------------------------------------------------------------
// Method Validation Functions
//------------------------------------------------------------------------------

/// Validate the init method signature
fn validate_init_method(method: &syn::ImplItemFn) -> syn::Result<()> {
    // Ensure first param is &mut self
    if method.sig.inputs.is_empty() || !matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Init method must take &mut self as first parameter",
        ));
    }

    // Ensure no other parameters
    if method.sig.inputs.len() > 1 {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Init method must not take any parameters other than &mut self",
        ));
    }

    // Validate return type
    if !matches!(method.sig.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Init method must not return a value",
        ));
    }

    Ok(())
}

/// Validate a request-response function signature
fn validate_request_response_function(method: &syn::ImplItemFn) -> syn::Result<()> {
    // Ensure first param is &mut self
    if method.sig.inputs.is_empty() || !matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Request-response handlers must take &mut self as their first parameter",
        ));
    }
    
    // No limit on additional parameters - we support any number
    // No validation for return type - any return type is allowed
    
    Ok(())
}

//------------------------------------------------------------------------------
// Method Analysis Functions
//------------------------------------------------------------------------------

/// Analyze the methods in an implementation block
fn analyze_methods(
    impl_block: &ItemImpl,
) -> syn::Result<(
    Option<syn::Ident>,          // init method
    Option<syn::Ident>,          // ws method (keeping for future)
    Vec<FunctionMetadata>,       // metadata for request/response methods
)> {
    let mut init_method = None;
    let mut ws_method = None;
    let mut function_metadata = Vec::new();

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            let ident = method.sig.ident.clone();
            
            // Check for method attributes
            let has_init = has_attribute(method, "init");
            let has_http = has_attribute(method, "http");
            let has_local = has_attribute(method, "local");
            let has_remote = has_attribute(method, "remote");
            let has_ws = has_attribute(method, "ws");

            // Handle init method
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

            // Handle WebSocket method
            if has_ws {
                if has_http || has_local || has_remote || has_init {
                    return Err(syn::Error::new_spanned(
                        method,
                        "#[ws] cannot be combined with other attributes",
                    ));
                }
                // TODO: Add proper validation when implementing ws support
                if ws_method.is_some() {
                    return Err(syn::Error::new_spanned(
                        method,
                        "Multiple #[ws] methods defined",
                    ));
                }
                ws_method = Some(ident);
                continue;
            }

            // Handle request-response methods
            if has_http || has_local || has_remote {
                validate_request_response_function(method)?;
                function_metadata.push(extract_function_metadata(
                    method, 
                    has_local, 
                    has_remote, 
                    has_http
                ));
            }
        }
    }

    Ok((init_method, ws_method, function_metadata))
}

/// Extract metadata from a function
fn extract_function_metadata(
    method: &syn::ImplItemFn,
    is_local: bool,
    is_remote: bool,
    is_http: bool,
) -> FunctionMetadata {
    let ident = method.sig.ident.clone();
    
    // Extract parameter types (skipping &mut self)
    let params = method.sig.inputs.iter()
        .skip(1)
        .filter_map(|input| {
            if let syn::FnArg::Typed(pat_type) = input {
                Some((*pat_type.ty).clone())
            } else {
                None
            }
        })
        .collect();

    // Extract return type
    let return_type = match &method.sig.output {
        ReturnType::Default => None, // () - no explicit return
        ReturnType::Type(_, ty) => Some((**ty).clone()),
    };

    // Create variant name (snake_case to CamelCase)
    let variant_name = to_camel_case(&ident.to_string());
    
    FunctionMetadata {
        name: ident,
        variant_name,
        params,
        return_type,
        is_async: method.sig.asyncness.is_some(),
        is_local,
        is_remote,
        is_http,
    }
}

//------------------------------------------------------------------------------
// Code Generation Functions
//------------------------------------------------------------------------------

/// Generate Request and Response enums based on function metadata
fn generate_request_response_enums(
    function_metadata: &[FunctionMetadata],
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    if function_metadata.is_empty() {
        return (quote! {}, quote! {});
    }

    // Request enum variants
    let request_variants = function_metadata.iter().map(|func| {
        let variant_name = format_ident!("{}", &func.variant_name);
        
        if func.params.is_empty() {
            // Unit variant for functions with no parameters
            quote! { #variant_name }
        } else if func.params.len() == 1 {
            // Simple tuple variant for single parameter
            let param_type = &func.params[0];
            quote! { #variant_name(#param_type) }
        } else {
            // Tuple variant with multiple types for multiple parameters
            let param_types = &func.params;
            quote! { #variant_name(#(#param_types),*) }
        }
    });

    // Response enum variants
    let response_variants = function_metadata.iter().map(|func| {
        let variant_name = format_ident!("{}", &func.variant_name);
        
        if let Some(return_type) = &func.return_type {
            let type_str = return_type.to_token_stream().to_string();
            if type_str == "()" {
                // Unit variant for () return type
                quote! { #variant_name }
            } else {
                // Tuple variant with return type
                quote! { #variant_name(#return_type) }
            }
        } else {
            // Unit variant for no explicit return
            quote! { #variant_name }
        }
    });

    // Generate the enum definitions with serialization derives
    (
        quote! {
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            enum Request {
                #(#request_variants),*
            }
        },
        quote! {
            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            enum Response {
                #(#response_variants),*
            }
        }
    )
}

/// Generate debug string representations of the Request and Response enums
fn generate_debug_enum_strings(
    function_metadata: &[FunctionMetadata],
) -> (String, String) {
    let debug_request_enum = function_metadata.iter().map(|func| {
        let variant_name = &func.variant_name;
        
        if func.params.is_empty() {
            format!("  {}", variant_name)
        } else if func.params.len() == 1 {
            let param_type = func.params[0].to_token_stream().to_string();
            format!("  {}({})", variant_name, param_type)
        } else {
            let param_types: Vec<_> = func.params.iter()
                .map(|ty| ty.to_token_stream().to_string())
                .collect();
            format!("  {}({})", variant_name, param_types.join(", "))
        }
    }).collect::<Vec<_>>().join("\n");
    
    let debug_response_enum = function_metadata.iter().map(|func| {
        let variant_name = &func.variant_name;
        
        if let Some(return_type) = &func.return_type {
            let type_str = return_type.to_token_stream().to_string();
            if type_str == "()" {
                format!("  {}", variant_name)
            } else {
                format!("  {}({})", variant_name, type_str)
            }
        } else {
            format!("  {}", variant_name)
        }
    }).collect::<Vec<_>>().join("\n");

    (debug_request_enum, debug_response_enum)
}

/// Generate handler match arms for request handling (works for local, remote, and HTTP)
fn generate_request_match_arms(
    handlers: &[&FunctionMetadata],
    self_ty: &Box<syn::Type>,
    handler_type: HandlerType,
) -> proc_macro2::TokenStream {
    if handlers.is_empty() {
        let message = match handler_type {
            HandlerType::Local => "No local handlers defined but received a local request",
            HandlerType::Remote => "No remote handlers defined but received a remote request",
            HandlerType::Http => "No HTTP handlers defined but received an HTTP request",
        };
        return quote! {
            hyperware_process_lib::logging::warn!(#message);
        };
    }

    let type_name = match handler_type {
        HandlerType::Local => "local",
        HandlerType::Remote => "remote",
        HandlerType::Http => "http",
    };

    // Create response handling code based on handler type
    let get_response_handling = |func: &FunctionMetadata, variant_name: &syn::Ident| -> proc_macro2::TokenStream {
        match handler_type {
            HandlerType::Local | HandlerType::Remote => {
                quote! {
                    let response = Response::#variant_name(result);
                    let resp = hyperware_process_lib::Response::new()
                        .body(serde_json::to_vec(&response).unwrap());
                    kiprintln!("Sending {} response: {:?}", #type_name, response);
                    resp.send().unwrap();
                }
            },
            HandlerType::Http => {
                quote! {
                    let response = Response::#variant_name(result);
                    let response_bytes = serde_json::to_vec(&response).unwrap();
                    kiprintln!("Sending HTTP response: {:?}", response);
                    hyperware_process_lib::http::server::send_response(
                        hyperware_process_lib::http::StatusCode::OK, 
                        None, 
                        response_bytes
                    );
                }
            }
        }
    };

    let dispatch_arms = handlers
        .iter()
        .map(|func| {
            let fn_name = &func.name;
            let variant_name = format_ident!("{}", &func.variant_name);
            
            // Get the appropriate response handling code
            let response_handling = get_response_handling(func, &variant_name);
            
            if func.is_async {
                // Async function handling using state pointer
                if func.params.is_empty() {
                    // Async function with no parameters
                    quote! {
                        Request::#variant_name => {
                            // Create a mutable reference that will be promoted to a static lifetime
                            // This is safe in WASM since we have a single-threaded environment
                            // and the state outlives all async operations
                            let state_ptr: *mut #self_ty = &mut state;
                            hyperware_app_common::hyper! {
                                // Inside the async block, we'll use the pointer to safely access state
                                let result = unsafe { (*state_ptr).#fn_name().await };
                                #response_handling
                            }
                        }
                    }
                } else if func.params.len() == 1 {
                    // Async function with a single parameter
                    quote! {
                        Request::#variant_name(param) => {
                            let param_captured = param;  // Capture param before moving into async block
                            // Create a mutable reference that will be promoted to a static lifetime
                            // This is safe in WASM since we have a single-threaded environment
                            // and the state outlives all async operations
                            let state_ptr: *mut #self_ty = &mut state;
                            hyperware_app_common::hyper! {
                                // Inside the async block, we'll use the pointer to safely access state
                                let result = unsafe { (*state_ptr).#fn_name(param_captured).await };
                                #response_handling
                            }
                        }
                    }
                } else {
                    // Async function with multiple parameters
                    let param_count = func.params.len();
                    let param_names = (0..param_count).map(|i| format_ident!("param{}", i));
                    let capture_statements = (0..param_count).map(|i| {
                        let param = format_ident!("param{}", i);
                        let captured = format_ident!("param{}_captured", i);
                        quote! { let #captured = #param; }
                    });
                    let captured_names = (0..param_count).map(|i| format_ident!("param{}_captured", i));
                    
                    quote! {
                        Request::#variant_name(#(#param_names),*) => {
                            // Capture all parameters before moving into async block
                            #(#capture_statements)*
                            // Create a mutable reference that will be promoted to a static lifetime
                            // This is safe in WASM since we have a single-threaded environment
                            // and the state outlives all async operations
                            let state_ptr: *mut #self_ty = &mut state;
                            hyperware_app_common::hyper! {
                                // Inside the async block, we'll use the pointer to safely access state
                                let result = unsafe { (*state_ptr).#fn_name(#(#captured_names),*).await };
                                #response_handling
                            }
                        }
                    }
                }
            } else {
                // Sync function handling
                if func.params.is_empty() {
                    quote! {
                        Request::#variant_name => {
                            let result = state.#fn_name();
                            #response_handling
                        }
                    }
                } else if func.params.len() == 1 {
                    quote! {
                        Request::#variant_name(param) => {
                            let result = state.#fn_name(param);
                            #response_handling
                        }
                    }
                } else {
                    let param_count = func.params.len();
                    let param_names = (0..param_count).map(|i| format_ident!("param{}", i));
                    let param_names2 = param_names.clone();
                    
                    quote! {
                        Request::#variant_name(#(#param_names),*) => {
                            let result = state.#fn_name(#(#param_names2),*);
                            #response_handling
                        }
                    }
                }
            }
        });
    
    // Add an explicit unreachable for other variants
    let unreachable_arm = quote! {
        _ => unreachable!(concat!("Non-", #type_name, " request variant received in ", #type_name, " handler"))
    };
    
    quote! {
        match request {
            #(#dispatch_arms)*
            #unreachable_arm
        }
    }
}

/// Generate local handler match arms for request handling
fn generate_local_request_match_arms(
    local_handlers: &[&FunctionMetadata],
    self_ty: &Box<syn::Type>,
) -> proc_macro2::TokenStream {
    generate_request_match_arms(local_handlers, self_ty, HandlerType::Local)
}

/// Generate remote handler match arms for request handling
fn generate_remote_request_match_arms(
    remote_handlers: &[&FunctionMetadata],
    self_ty: &Box<syn::Type>,
) -> proc_macro2::TokenStream {
    generate_request_match_arms(remote_handlers, self_ty, HandlerType::Remote)
}

/// Generate HTTP handler match arms for request handling
fn generate_http_request_match_arms(
    http_handlers: &[&FunctionMetadata],
    self_ty: &Box<syn::Type>,
) -> proc_macro2::TokenStream {
    generate_request_match_arms(http_handlers, self_ty, HandlerType::Http)
}

/// Generate a debug string representation of a handler dispatch code
fn generate_debug_handler_string(
    handlers: &[&FunctionMetadata],
    handler_type: &str,
) -> String {
    if handlers.is_empty() {
        return format!("// No {} handlers defined", handler_type).to_string();
    }

    let debug_cases = handlers
        .iter()
        .map(|func| {
            let fn_name = &func.name;
            let variant_name = &func.variant_name;
            let async_keyword = if func.is_async { "async " } else { "" };
            
            if func.params.is_empty() {
                format!("    Request::{} => {{ /* Call state.{}{}() */ }}", 
                    variant_name, async_keyword, fn_name)
            } else if func.params.len() == 1 {
                format!("    Request::{}(param) => {{ /* Call state.{}{}(param) */ }}", 
                    variant_name, async_keyword, fn_name)
            } else {
                let param_count = func.params.len();
                let param_names: Vec<_> = (0..param_count).map(|i| format!("param{}", i)).collect();
                let params_list = param_names.join(", ");
                
                format!("    Request::{}({}) => {{ /* Call state.{}{}({}) */ }}", 
                    variant_name, params_list, async_keyword, fn_name, params_list)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
        
    format!("match request {{\n{}\n}}", debug_cases)
}

/// Generate a debug string representation of the local handler dispatch code
fn generate_debug_local_handler_string(
    local_handlers: &[&FunctionMetadata],
) -> String {
    generate_debug_handler_string(local_handlers, "local")
}

/// Generate a debug string representation of the remote handler dispatch code
fn generate_debug_remote_handler_string(
    remote_handlers: &[&FunctionMetadata],
) -> String {
    generate_debug_handler_string(remote_handlers, "remote")
}

/// Generate a debug string representation of the HTTP handler dispatch code
fn generate_debug_http_handler_string(
    http_handlers: &[&FunctionMetadata],
) -> String {
    generate_debug_handler_string(http_handlers, "HTTP")
}

/// Remove our custom attributes from the implementation block
fn clean_impl_block(impl_block: &ItemImpl) -> ItemImpl {
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
    cleaned_impl_block
}

//------------------------------------------------------------------------------
// Main Macro Implementation
//------------------------------------------------------------------------------

/// The main procedural macro
#[proc_macro_attribute]
pub fn hyperprocess(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input
    let attr_args = parse_macro_input!(attr as MetaList);
    let impl_block = parse_macro_input!(item as ItemImpl);

    // Parse the macro arguments
    let args = match parse_args(attr_args) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    // Get the self type from the implementation block
    let self_ty = &impl_block.self_ty;

    // Analyze the methods in the implementation block
    let (init_method, ws_method, function_metadata) =
        match analyze_methods(&impl_block) {
            Ok(methods) => methods,
            Err(e) => return e.to_compile_error().into(),
        };

    // Split functions by handler type
    let local_handlers: Vec<_> = function_metadata.iter()
        .filter(|f| f.is_local)
        .collect();
    
    let remote_handlers: Vec<_> = function_metadata.iter()
        .filter(|f| f.is_remote)
        .collect();
    
    let http_handlers: Vec<_> = function_metadata.iter()
        .filter(|f| f.is_http)
        .collect();

    // Generate Request and Response enums
    let (request_enum, response_enum) = generate_request_response_enums(&function_metadata);
    
    // Generate debug strings
    let (debug_request_enum, debug_response_enum) = generate_debug_enum_strings(&function_metadata);
    
    // Generate handler match arms
    let local_request_match_arms = generate_local_request_match_arms(&local_handlers, self_ty);
    let remote_request_match_arms = generate_remote_request_match_arms(&remote_handlers, self_ty);
    let http_request_match_arms = generate_http_request_match_arms(&http_handlers, self_ty);
    
    // Generate debug handler strings
    let debug_local_handler_dispatch = generate_debug_local_handler_string(&local_handlers);
    let debug_remote_handler_dispatch = generate_debug_remote_handler_string(&remote_handlers);
    let debug_http_handler_dispatch = generate_debug_http_handler_string(&http_handlers);

    // Clean the implementation block
    let cleaned_impl_block = clean_impl_block(&impl_block);

    // Export the init method identifier for direct use in the component implementation
    let init_method_ident = if let Some(method_name) = &init_method {
        quote! { Some(stringify!(#method_name)) }
    } else {
        quote! { None::<&str> }
    };

    // For direct method call, we need the actual method identifier
    let init_method_call = if let Some(method_name) = &init_method {
        quote! { state.#method_name(); }
    } else {
        quote! {}
    };

    // Extract values from args for use in the quote macro
    let name = &args.name;
    let endpoints = &args.endpoints;
    let save_config = &args.save_config;
    let wit_world = &args.wit_world;
    
    let icon = match &args.icon {
        Some(icon_str) => quote! { Some(#icon_str.to_string()) },
        None => quote! { None }
    };
    
    let widget = match &args.widget {
        Some(widget_str) => quote! { Some(#widget_str.to_string()) },
        None => quote! { None }
    };
    
    let ui = match &args.ui {
        Some(ui_expr) => quote! { Some(#ui_expr) },
        None => quote! { None }
    };

    // Generate the final output
    let output = quote! {
        wit_bindgen::generate!({
            path: "target/wit",
            world: #wit_world,
            generate_unused_types: true,
            additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
        });

        use hyperware_process_lib::http::server::HttpBindingConfig;
        use hyperware_app_common::Binding;

        #cleaned_impl_block

        // Add our generated request/response enums
        #request_enum
        #response_enum

        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                // Debug: Print the generated enum definitions in a clean format
                kiprintln!("============= GENERATED REQUEST ENUM =============");
                kiprintln!("enum Request {{");
                kiprintln!("{}", #debug_request_enum);
                kiprintln!("}}");
                
                kiprintln!("============= GENERATED RESPONSE ENUM ============");
                kiprintln!("enum Response {{");
                kiprintln!("{}", #debug_response_enum);
                kiprintln!("}}");
                
                // Debug: Print the handler dispatch code
                kiprintln!("============= LOCAL HANDLER DISPATCH =============");
                kiprintln!("// Pseudo-code representation of generated handler:");
                kiprintln!("fn handle_local_request(state: &mut State, request: Request) {{");
                kiprintln!("{}", #debug_local_handler_dispatch);
                kiprintln!("}}");
                
                kiprintln!("============= REMOTE HANDLER DISPATCH ============");
                kiprintln!("// Pseudo-code representation of generated handler:");
                kiprintln!("fn handle_remote_request(state: &mut State, message: &Message, request: Request) {{");
                kiprintln!("{}", #debug_remote_handler_dispatch);
                kiprintln!("}}");
                
                kiprintln!("============= HTTP HANDLER DISPATCH ==============");
                kiprintln!("// Pseudo-code representation of generated handler:");
                kiprintln!("fn handle_http_request(state: &mut State, request: Request) {{");
                kiprintln!("{}", #debug_http_handler_dispatch);
                kiprintln!("}}");
                
                kiprintln!("Starting application...");
                
                // Initialize our state
                let mut state = hyperware_app_common::initialize_state::<#self_ty>();
                
                // Set up necessary components
                let app_name = #name;
                let app_icon = #icon;
                let app_widget = #widget;
                let ui_config = #ui;
                let endpoints = #endpoints;
                
                // Setup UI if needed
                if app_icon.is_some() && app_widget.is_some() {
                    hyperware_process_lib::homepage::add_to_homepage(app_name, app_icon, Some("/"), app_widget);
                }
                
                // Initialize logging
                hyperware_process_lib::logging::init_logging(
                    hyperware_process_lib::logging::Level::DEBUG,
                    hyperware_process_lib::logging::Level::INFO,
                    None, Some((0, 0, 1, 1)), None
                ).unwrap();
                
                // Setup server with endpoints
                let mut server = hyperware_app_common::setup_server(ui_config.as_ref(), &endpoints);
                
                // Initialize app state
                if #init_method_ident.is_some() {
                    #init_method_call
                }
                
                // Main event loop
                loop {
                    hyperware_app_common::APP_CONTEXT.with(|ctx| {
                        ctx.borrow_mut().executor.poll_all_tasks();
                    });
                    
                    match hyperware_process_lib::await_message() {
                        Ok(message) => {
                            // Check if this is an HTTP message from the HTTP server
                            if message.is_local() && message.source().process == "http-server:distro:sys" {
                                // Parse HTTP server request
                                match serde_json::from_slice::<hyperware_process_lib::http::server::HttpServerRequest>(message.body()) {
                                    Ok(http_server_request) => {
                                        match http_server_request {
                                            hyperware_process_lib::http::server::HttpServerRequest::Http(_) => {
                                                // Get the blob containing the actual request
                                                let Some(blob) = message.blob() else {
                                                    hyperware_process_lib::logging::warn!("Failed to get blob for HTTP, sending BAD_REQUEST");
                                                    hyperware_process_lib::http::server::send_response(
                                                        hyperware_process_lib::http::StatusCode::BAD_REQUEST, 
                                                        None, 
                                                        vec![]
                                                    );
                                                    return;
                                                };
                                                
                                                // Process HTTP request
                                                match serde_json::from_slice::<serde_json::Value>(blob.bytes()) {
                                                    Ok(req_value) => {
                                                        match serde_json::from_value::<Request>(req_value.clone()) {
                                                            Ok(request) => {
                                                                // Handle the HTTP request
                                                                #http_request_match_arms
                                                                
                                                                // Save state if needed
                                                                hyperware_app_common::maybe_save_state(&state);
                                                            },
                                                            Err(e) => {
                                                                hyperware_process_lib::logging::warn!("Failed to deserialize HTTP request into Request enum: {}", e);
                                                                hyperware_process_lib::http::server::send_response(
                                                                    hyperware_process_lib::http::StatusCode::BAD_REQUEST, 
                                                                    None, 
                                                                    format!("Invalid request format: {}", e).into_bytes()
                                                                );
                                                            }
                                                        }
                                                    },
                                                    Err(e) => {
                                                        hyperware_process_lib::logging::warn!("Failed to parse HTTP request as JSON: {}", e);
                                                        hyperware_process_lib::http::server::send_response(
                                                            hyperware_process_lib::http::StatusCode::BAD_REQUEST, 
                                                            None, 
                                                            format!("Invalid JSON: {}", e).into_bytes()
                                                        );
                                                    }
                                                }
                                            },
                                            hyperware_process_lib::http::server::HttpServerRequest::WebSocketPush { channel_id, message_type } => {
                                                // TODO: Handle WebSocketPush
                                                hyperware_process_lib::logging::info!("WebSocketPush not yet implemented");
                                            },
                                            hyperware_process_lib::http::server::HttpServerRequest::WebSocketOpen { path, channel_id } => {
                                                // TODO: Handle WebSocketOpen
                                                hyperware_process_lib::logging::info!("WebSocketOpen not yet implemented");
                                            },
                                            hyperware_process_lib::http::server::HttpServerRequest::WebSocketClose(channel_id) => {
                                                // TODO: Handle WebSocketClose
                                                hyperware_process_lib::logging::info!("WebSocketClose not yet implemented");
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        hyperware_process_lib::logging::warn!("Failed to parse HTTP server request: {}", e);
                                    }
                                }
                            } else if message.is_local() {
                                // Regular local message handling
                                match serde_json::from_slice::<serde_json::Value>(message.body()) {
                                    Ok(req_value) => {
                                        // Process the local request based on our handlers
                                        match serde_json::from_value::<Request>(req_value.clone()) {
                                            Ok(request) => {
                                                // Match on the request variant and call the appropriate handler
                                                #local_request_match_arms
                                                
                                                // Save state if needed
                                                hyperware_app_common::maybe_save_state(&state);
                                            },
                                            Err(e) => {
                                                hyperware_process_lib::logging::warn!("Failed to deserialize local request into Request enum: {}", e);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        hyperware_process_lib::logging::warn!("Failed to parse message body as JSON: {}", e);
                                    }
                                }
                            } else {
                                // Remote message handling
                                match serde_json::from_slice::<serde_json::Value>(message.body()) {
                                    Ok(req_value) => {
                                        // Process the remote request based on our handlers
                                        match serde_json::from_value::<Request>(req_value.clone()) {
                                            Ok(request) => {
                                                // Match on the request variant and call the appropriate handler
                                                #remote_request_match_arms
                                                
                                                // Save state if needed
                                                hyperware_app_common::maybe_save_state(&state);
                                            },
                                            Err(e) => {
                                                hyperware_process_lib::logging::warn!("Failed to deserialize remote request into Request enum: {}", e);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        hyperware_process_lib::logging::warn!("Failed to parse message body as JSON: {}", e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            // We'll improve error handling later
                            kiprintln!("Failed to await message: {}", e);
                        }
                    }
                }
            }
        }

        export!(Component);
    };

    output.into()
}