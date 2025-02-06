use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Parse, parse::ParseStream, parse_macro_input, Block, Expr, Expr::Call as ExprCallNode,
    Expr::Path as ExprPathNode, ExprCall, ExprPath, Ident, Result, Token, Type,
};

/// The main macro entry point
#[proc_macro]
pub fn send_async(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as SendAsyncInvocation);
    invocation.expand_macro().into()
}

/// Our data structure representing the macro invocation.
struct SendAsyncInvocation {
    destination: Expr,
    request_expr: Expr,

    /// Optional callback
    callback: Option<Callback>,

    /// Optional timeout expression
    timeout: Option<Expr>,

    /// Optional on-timeout block
    on_timeout_block: Option<Block>,
}

/// Holds the pieces of `(resp_ident, st_ident: st_type) { callback_block }`.
struct Callback {
    resp_ident: Ident,
    st_ident: Ident,
    st_type: Type,
    callback_block: Block,
}

impl Parse for SendAsyncInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        // 1) Parse the required parts: destination expr, request expr
        let destination: Expr = input.parse()?;
        input.parse::<Token![,]>()?;

        let request_expr: Expr = input.parse()?;

        // 2) Look ahead for optional pieces (callback / timeout / on_timeout)
        let mut callback: Option<Callback> = None;
        let mut timeout: Option<Expr> = None;
        let mut on_timeout_block: Option<Block> = None;

        // If there's a trailing comma, consume it and parse further
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;

            while !input.is_empty() {
                // If we see `(` => parse (resp, st: MyState) { ... }
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let resp_ident: Ident = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let st_ident: Ident = content.parse()?;
                    content.parse::<Token![:]>()?;
                    let st_type: Type = content.parse()?;

                    // Parse the callback block: `{ ... }`
                    let callback_block: Block = input.parse()?;

                    callback = Some(Callback {
                        resp_ident,
                        st_ident,
                        st_type,
                        callback_block,
                    });

                    // Check if there's another trailing comma
                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                } else if input.peek(syn::LitInt) || input.peek(syn::Lit) || input.peek(syn::Ident)
                {
                    // Probably the timeout expression
                    if timeout.is_none() {
                        timeout = Some(input.parse()?);
                    } else {
                        // If we already have a timeout, break or error
                        break;
                    }

                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                } else if input.peek(Ident) {
                    // Possibly `on_timeout => { ... }`
                    let ident: Ident = input.parse()?;
                    if ident == "on_timeout" {
                        input.parse::<Token![=>]>()?;
                        let block: Block = input.parse()?;
                        on_timeout_block = Some(block);
                    } else {
                        return Err(syn::Error::new_spanned(
                            ident,
                            "Expected `on_timeout => { ... }` or a valid expression.",
                        ));
                    }

                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                } else {
                    break;
                }
            }
        }

        Ok(SendAsyncInvocation {
            destination,
            request_expr,
            callback,
            timeout,
            on_timeout_block,
        })
    }
}

impl SendAsyncInvocation {
    fn expand_macro(&self) -> proc_macro2::TokenStream {
        // 1) Identify the variant name from request (e.g. Foo in SomeRequest::Foo(...))
        let variant_ident = extract_variant_name(&self.request_expr)
            .unwrap_or_else(|| syn::Ident::new("UNKNOWN_VARIANT", proc_macro2::Span::call_site()));

        // 2) Build "response path" => rewrite SomethingRequest -> SomethingResponse
        let response_path = build_response_path(&self.request_expr);

        // 3) We'll match on response_path::Variant(inner) => inner
        let success_arm = quote! {
            #response_path :: #variant_ident(inner) => inner
        };

        // 4) Default timeout is 30
        let timeout_expr = self
            .timeout
            .clone()
            .unwrap_or_else(|| syn::parse_str("30").unwrap());

        // 5) on_timeout block is optional => `Option<Box<...>>`
        let on_timeout_code = if let Some(ref block) = self.on_timeout_block {
            quote! {
                Some(Box::new(move |any_state: &mut dyn std::any::Any| {
                    #block
                    Ok(())
                }))
            }
        } else {
            quote! { None }
        };

        // 6) The user's callback is optional. But the downstream code expects
        //    a *non-optional* `Box<dyn FnOnce(...) -> Result<_, _>>`.
        //    So if we have no callback, produce a no-op closure.
        let on_success_code = match &self.callback {
            Some(cb) => {
                let Callback {
                    resp_ident,
                    st_ident,
                    st_type,
                    callback_block,
                } = cb;

                quote! {
                    Box::new(move |resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                        let parsed = ::serde_json::from_slice::<#response_path>(resp_bytes)
                            .map_err(|e| anyhow::anyhow!("Failed to deserialize response: {}", e))?;

                        let #resp_ident = match parsed {
                            #success_arm,
                            other => {
                                return Err(anyhow::anyhow!(
                                    "Got the wrong variant (expected {}) => got: {:?}",
                                    stringify!(#variant_ident),
                                    other
                                ));
                            }
                        };

                        let #st_ident = any_state
                            .downcast_mut::<#st_type>()
                            .ok_or_else(|| anyhow::anyhow!("Downcast user state failed!"))?;

                        #callback_block
                        Ok(())
                    })
                }
            }
            None => {
                // Produce a no-op closure
                quote! {
                    Box::new(move |_resp_bytes: &[u8], _any_state: &mut dyn std::any::Any| {
                        // No callback was provided, do nothing
                        Ok(())
                    })
                }
            }
        };

        // 7) We always insert a PendingCallback, because your "PendingCallback"
        //    struct expects on_success: Box<...> (not Option). The "on_timeout" is optional.
        let dest_expr = &self.destination;
        let request_expr = &self.request_expr;

        quote! {
            {
                // Serialize the request
                match ::serde_json::to_vec(&#request_expr) {
                    Ok(b) => {
                        let correlation_id = ::uuid::Uuid::new_v4().to_string();

                        // Insert callback into hidden state
                        ::kinode_app_common::HIDDEN_STATE.with(|cell| {
                            let mut hs = cell.borrow_mut();
                            hs.as_mut().map(|state| {
                                state.pending_callbacks.insert(
                                    correlation_id.clone(),
                                    kinode_app_common::PendingCallback {
                                        on_success: #on_success_code,
                                        on_timeout: #on_timeout_code,
                                    }
                                )
                            });
                        });

                        // Actually send
                        let _ = ::kinode_process_lib::Request::to(#dest_expr)
                            .context(correlation_id.as_bytes())
                            .body(b)
                            .expects_response(#timeout_expr)
                            .send();
                    },
                    Err(e) => {
                        ::kinode_process_lib::kiprintln!("Error serializing request: {}", e);
                    }
                }
                // Explicitly return unit to make this a statement
                ()
            }
        }
    }
}

/// Extract the final variant name from e.g. `SomeRequest::Foo(...)` or `SomeRequest::Foo`.
fn extract_variant_name(expr: &Expr) -> Option<Ident> {
    match expr {
        // Handle call-style variants: SomeRequest::Foo(...)
        ExprCallNode(ExprCall { func, .. }) => {
            if let ExprPathNode(ExprPath { path, .. }) = &**func {
                path.segments.last().map(|seg| seg.ident.clone())
            } else {
                None
            }
        }
        // Handle unit variants: SomeRequest::Foo
        ExprPathNode(ExprPath { path, .. }) => {
            path.segments.last().map(|seg| seg.ident.clone())
        }
        _ => None,
    }
}

/// Build a "response path" by rewriting `XyzRequest -> XyzResponse`.
fn build_response_path(expr: &Expr) -> proc_macro2::TokenStream {
    let path = match expr {
        // Handle call-style variants: SomeRequest::Foo(...)
        ExprCallNode(ExprCall { func, .. }) => {
            if let ExprPathNode(ExprPath { path, .. }) = &**func {
                path
            } else {
                return quote! { compile_error!("Expected path expression") };
            }
        }
        // Handle unit variants: SomeRequest::Foo
        ExprPathNode(ExprPath { path, .. }) => path,
        _ => return quote! { compile_error!("Expected path or call expression") },
    };

    let segments = &path.segments;
    if segments.len() < 2 {
        return quote! { compile_error!("Invalid request type: expected path with at least two segments") };
    }

    let enum_seg = &segments[segments.len() - 2];

    // Convert e.g. "FooRequest" => "FooResponse"
    let old_ident_str = enum_seg.ident.to_string();
    let new_ident_str = if let Some(base) = old_ident_str.strip_suffix("Request") {
        format!("{}Response", base)
    } else {
        format!("{}Response", old_ident_str)
    };
    let new_ident = syn::Ident::new(&new_ident_str, enum_seg.ident.span());

    // Rebuild the path
    let mut new_segments = syn::punctuated::Punctuated::new();
    for i in 0..segments.len() - 2 {
        new_segments.push(segments[i].clone());
    }
    let mut replaced_seg = enum_seg.clone();
    replaced_seg.ident = new_ident;
    new_segments.push(replaced_seg);

    let new_path = syn::Path {
        leading_colon: path.leading_colon,
        segments: new_segments,
    };
    quote! { #new_path }
}
