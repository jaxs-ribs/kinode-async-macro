use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_macro_input, spanned::Spanned, Expr, ItemImpl, Meta, ReturnType};

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

// Function metadata structure
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

// Implement Parse for our newtype wrapper
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

// Helper function to convert snake_case to CamelCase
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

// Validation function for init method
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
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Init method must not return a value",
        ));
    }

    Ok(())
}

// Validation function for our request-response style functions
fn validate_request_response_function(method: &syn::ImplItemFn) -> syn::Result<()> {
    // Ensure first param is &mut self
    if method.sig.inputs.is_empty() || !matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "Request-response handlers must take &mut self as their first parameter",
        ));
    }

    // No limit on additional parameters anymore - we support any number
    
    // No validation for return type - any return type is allowed
    
    Ok(())
}

// Analyze methods, validating and collecting metadata
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

            // We're ignoring WS handlers for now, but keeping the placeholder
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
                
                // Extract parameter types (skipping &mut self)
                let mut params = Vec::new();
                for input in method.sig.inputs.iter().skip(1) {
                    if let syn::FnArg::Typed(pat_type) = input {
                        params.push((*pat_type.ty).clone());
                    }
                }

                // Extract return type
                let return_type = match &method.sig.output {
                    syn::ReturnType::Default => None, // () - no explicit return
                    syn::ReturnType::Type(_, ty) => Some((**ty).clone()),
                };

                // Check if function is async
                let is_async = method.sig.asyncness.is_some();

                // Create variant name (snake_case to CamelCase)
                let variant_name = to_camel_case(&ident.to_string());
                
                // Create function metadata and add to collection
                function_metadata.push(FunctionMetadata {
                    name: ident.clone(),
                    variant_name,
                    params,
                    return_type,
                    is_async,
                    is_local: has_local,
                    is_remote: has_remote,
                    is_http: has_http,
                });
            }
        }
    }

    Ok((init_method, ws_method, function_metadata))
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

    // Analyze methods, only collecting init, ws and our new style handlers
    let (init_method, ws_method, function_metadata) =
        match analyze_methods(&impl_block) {
            Ok(methods) => methods,
            Err(e) => return e.to_compile_error().into(),
        };

    // Generate Request and Response enums
    let (request_enum, response_enum) = if !function_metadata.is_empty() {
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
    } else {
        // No function metadata, so no enums needed
        (quote! {}, quote! {})
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
        
    // Generate a nice clean representation of the enums for debug printing
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

    // Generate local handler dispatch code
    let local_handler_code = if !local_handlers.is_empty() {
        let dispatch_arms = local_handlers
            .iter()
            .map(|func| {
                let fn_name = &func.name;
                let variant_name = format_ident!("{}", &func.variant_name);
                
                if func.params.is_empty() {
                    // Function with no parameters
                    quote! {
                        Request::#variant_name => {
                            let result = state.#fn_name();
                            let response = Response::#variant_name(result);
                            // TODO: Send response back
                        }
                    }
                } else if func.params.len() == 1 {
                    // Function with a single parameter
                    quote! {
                        Request::#variant_name(param) => {
                            let result = state.#fn_name(param);
                            let response = Response::#variant_name(result);
                            // TODO: Send response back
                        }
                    }
                } else {
                    // Function with multiple parameters
                    // Create parameter names (param0, param1, etc.)
                    let param_count = func.params.len();
                    let param_names = (0..param_count).map(|i| format_ident!("param{}", i));
                    let param_names_2 = param_names.clone(); // Clone for reuse
                    
                    quote! {
                        Request::#variant_name(#(#param_names),*) => {
                            let result = state.#fn_name(#(#param_names_2),*);
                            let response = Response::#variant_name(result);
                            // TODO: Send response back
                        }
                    }
                }
            });
        
        quote! {
            |message: &Message, state: &mut #self_ty, req: serde_json::Value| {
                match serde_json::from_value::<Request>(req) {
                    Ok(request) => {
                        match request {
                            #(#dispatch_arms)*
                        }
                    },
                    Err(e) => {
                        warn!("Failed to deserialize local request into Request enum: {}", e);
                    }
                }
            }
        }
    } else {
        quote! { no_local_request }
    };

    // Generate remote handler dispatch code (similar to local)
    let remote_handler_code = if !remote_handlers.is_empty() {
        // Similar implementation to local_handler_code
        quote! { no_remote_request }  // Placeholder for now
    } else {
        quote! { no_remote_request }
    };

    // Generate HTTP handler dispatch code (similar to local)
    let http_handler_code = if !http_handlers.is_empty() {
        // Similar implementation to local_handler_code
        quote! { no_http_api_call }  // Placeholder for now
    } else {
        quote! { no_http_api_call }
    };

    // Generate init function code
    let init_fn_code = if let Some(method_name) = init_method {
        quote! { |state: &mut #self_ty| state.#method_name() }
    } else {
        quote! { no_init_fn }
    };

    // Generate ws handler code
    let ws_handler_code = if let Some(_) = ws_method {
        // We'll implement this later
        quote! { no_ws_handler }
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

        // Add our generated request/response enums
        #request_enum
        #response_enum

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
        }
        
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
                
                // This dummy implementation is just to make the compiler happy
                // while we work on the macro
                kiprintln!("Dummy implementation - not actually running the app");
                
                // This ensures the compiler knows about our generated enums
                // so that type checking can work
                if false {
                    let _: Request = unsafe { std::mem::zeroed() };
                    let _: Response = unsafe { std::mem::zeroed() };
                }
            }
        }

        export!(Component);
    };

    output.into()
}