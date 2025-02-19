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

fn validate_method_signature(method: &syn::ImplItemFn, spec: MethodSignatureSpec) -> syn::Result<()> {
    // Validate parameter count
    if method.sig.inputs.len() != spec.param_count {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!("{} handler must take {} parameters", spec.handler_name, spec.param_count)
        ));
    }

    // Skip first parameter (assumed to be &mut self)
    for (idx, (expected_type, error_msg)) in spec.param_types.iter().enumerate() {
        match &method.sig.inputs[idx + 1] {
            syn::FnArg::Typed(pat) if pat.ty.as_ref().to_token_stream().to_string() == *expected_type => {}
            _ => return Err(syn::Error::new_spanned(&method.sig.inputs[idx + 1], error_msg))
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
    let spec = MethodSignatureSpec {
        param_count: 3,
        param_types: vec![
            ("& str", "Second parameter must be &str"),
        ],
        handler_name: "HTTP",
    };
    validate_method_signature(method, spec)
}

fn validate_message_handler(method: &syn::ImplItemFn, handler_name: &str) -> syn::Result<()> {
    let spec = MethodSignatureSpec {
        param_count: 4,
        param_types: vec![
            ("& Message", "Second parameter must be &Message"),
            ("& mut HttpServer", "Third parameter must be &mut HttpServer"),
        ],
        handler_name: match handler_name {
            "Local" => "Local",
            "Remote" => "Remote",
            _ => "Message", // fallback, though this shouldn't happen
        },
    };
    validate_method_signature(method, spec)
}

fn validate_ws_method(method: &syn::ImplItemFn) -> syn::Result<()> {
    let spec = MethodSignatureSpec {
        param_count: 5,
        param_types: vec![
            ("& mut HttpServer", "Second parameter must be &mut HttpServer"),
            ("u32", "Third parameter must be u32"),
            ("WsMessageType", "Fourth parameter must be WsMessageType"),
            ("LazyLoadBlob", "Fifth parameter must be LazyLoadBlob"),
        ],
        handler_name: "WS",
    };
    validate_method_signature(method, spec)
}

fn analyze_methods(impl_block: &ItemImpl) -> syn::Result<(
    Option<syn::Ident>,
    Option<syn::Ident>,
    Option<syn::Ident>,
    Option<syn::Ident>,
    Option<syn::Ident>,
)> {
    let mut init_method = None;
    let mut http_method = None;
    let mut local_method = None;
    let mut remote_method = None;
    let mut ws_method = None;

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            for attr in &method.attrs {
                let ident = method.sig.ident.clone();
                
                if attr.path().is_ident("init") {
                    if init_method.is_some() {
                        return Err(syn::Error::new_spanned(attr, "Multiple #[init] methods defined"));
                    }
                    validate_init_method(method)?;
                    init_method = Some(ident);
                } else if attr.path().is_ident("http") {
                    if http_method.is_some() {
                        return Err(syn::Error::new_spanned(attr, "Multiple #[http] methods defined"));
                    }
                    validate_http_method(method)?;
                    http_method = Some(ident);
                } else if attr.path().is_ident("local") {
                    if local_method.is_some() {
                        return Err(syn::Error::new_spanned(attr, "Multiple #[local] methods defined"));
                    }
                    validate_message_handler(method, "Local")?;
                    local_method = Some(ident);
                } else if attr.path().is_ident("remote") {
                    if remote_method.is_some() {
                        return Err(syn::Error::new_spanned(attr, "Multiple #[remote] methods defined"));
                    }
                    validate_message_handler(method, "Remote")?;
                    remote_method = Some(ident);
                } else if attr.path().is_ident("ws") {
                    if ws_method.is_some() {
                        return Err(syn::Error::new_spanned(attr, "Multiple #[ws] methods defined"));
                    }
                    validate_ws_method(method)?;
                    ws_method = Some(ident);
                }
            }
        }
    }

    Ok((init_method, http_method, local_method, remote_method, ws_method))
}

#[proc_macro_attribute]
pub fn hyperprocess(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(attr as MetaList);
    let mut impl_block = parse_macro_input!(item as ItemImpl);

    let args = match parse_args(attr_args) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    let self_ty = &impl_block.self_ty;

    let (init_method, http_method, local_method, remote_method, ws_method) =
        match analyze_methods(&impl_block) {
            Ok(methods) => methods,
            Err(e) => return e.to_compile_error().into(),
        };

    let init_fn_code = if let Some(method_name) = init_method {
        quote! { |state: &mut #self_ty| state.#method_name() }
    } else {
        quote! { no_init_fn }
    };

    let handle_http_code = if let Some(method_name) = http_method {
        quote! { |state: &mut #self_ty, path: &str, req| state.#method_name(path, req) }
    } else {
        quote! { no_http_api_call }
    };

    let handle_local_code = if let Some(method_name) = local_method {
        quote! { |message: &Message, state: &mut #self_ty, server: &mut HttpServer, req| state.#method_name(message, server, req) }
    } else {
        quote! { no_local_request }
    };

    let handle_remote_code = if let Some(method_name) = remote_method {
        quote! { |message: &Message, state: &mut #self_ty, server: &mut HttpServer, req| state.#method_name(message, server, req) }
    } else {
        quote! { no_remote_request }
    };

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

        struct Component;
        impl Guest for Component {
            fn init(_our: String) {
                use hyperware_app_common::prelude::*;
                use hyperware_app_common::{
                    app,
                    no_init_fn,
                    no_http_api_call,
                    no_local_request,
                    no_remote_request,
                    no_ws_handler
                };
                use hyperware_process_lib::kiprintln;

                let init_fn = #init_fn_code;
                let handle_http = #handle_http_code;
                let handle_local = #handle_local_code;
                let handle_remote = #handle_remote_code;
                let handle_ws = #handle_ws_code;

                let endpoints_vec = #endpoints;

                let closure = app(
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
