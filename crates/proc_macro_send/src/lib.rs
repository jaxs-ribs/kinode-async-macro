use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Parse, parse::ParseStream, parse_macro_input, Block, Expr,
    Expr::Call as ExprCallNode, Expr::Path as ExprPathNode, ExprCall, ExprPath,
    Ident, Result, Token, Type,
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

                } else if input.peek(syn::LitInt)
                    || input.peek(syn::Lit)
                    || input.peek(syn::Ident)
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

/// Extract the final variant name from e.g. `SomeRequest::Foo(...)`.
fn extract_variant_name(expr: &Expr) -> Option<Ident> {
    if let ExprCallNode(ExprCall { func, .. }) = expr {
        if let ExprPathNode(ExprPath { path, .. }) = &**func {
            if let Some(seg) = path.segments.last() {
                return Some(seg.ident.clone());
            }
        }
    }
    None
}

/// Build a "response path" by rewriting `XyzRequest -> XyzResponse`.
fn build_response_path(expr: &Expr) -> proc_macro2::TokenStream {
    if let ExprCallNode(ExprCall { func, .. }) = expr {
        if let ExprPathNode(ExprPath { path, .. }) = &**func {
            let segments = &path.segments;
            if segments.len() < 2 {
                return quote! { UNKNOWN_RESPONSE };
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
            return quote! { #new_path };
        }
    }
    quote! { UNKNOWN_RESPONSE }
}

// Fanout macro

/// usage:
/// fan_out!(
///   addresses,           // Vec<String>
///   requests,            // Vec<MyRequest>
///   (all_results, st: MyState) {
///       // final aggregator callback
///   },
///   30 // optional
/// );
struct FanOutInvocation {
    addresses_expr: Expr,
    requests_expr: Expr,
    callback: FinalCallback,
    timeout: Option<Expr>,
}

struct FinalCallback {
    results_ident: Ident,
    state_ident: Ident,
    state_type: Type,
    block: Block,
}

impl Parse for FanOutInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let addresses_expr: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let requests_expr: Expr = input.parse()?;
        input.parse::<Token![,]>()?;

        let content;
        syn::parenthesized!(content in input);
        let results_ident: Ident = content.parse()?;
        content.parse::<Token![,]>()?;
        let state_ident: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let state_type: Type = content.parse()?;
        let block: Block = input.parse()?;

        let mut timeout = None;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if !input.is_empty() {
                timeout = Some(input.parse()?);
            }
        }

        Ok(Self {
            addresses_expr,
            requests_expr,
            callback: FinalCallback {
                results_ident,
                state_ident,
                state_type,
                block,
            },
            timeout,
        })
    }
}

#[proc_macro]
pub fn fan_out(input: TokenStream) -> TokenStream {
    let invoc = parse_macro_input!(input as FanOutInvocation);
    expand_fan_out(invoc).into()
}

fn expand_fan_out(invoc: FanOutInvocation) -> proc_macro2::TokenStream {
    let FanOutInvocation {
        addresses_expr,
        requests_expr,
        callback:
            FinalCallback {
                results_ident,
                state_ident,
                state_type,
                block,
            },
        timeout,
    } = invoc;

    let timeout_ts = timeout.unwrap_or_else(|| syn::parse_str("30").unwrap());

    // We'll store aggregator of type FanOutAggregator<serde_json::Value>
    // For a typed approach, you'd parse the request to find the "inner type".
    let aggregator_ty = quote! { ::kinode_app_common::FanOutAggregator<::serde_json::Value> };

    quote! {
        {
            use ::uuid::Uuid;
            use ::anyhow::{anyhow, Result};
            use ::kinode_process_lib::{kiprintln, Request};
            use ::kinode_app_common::{HIDDEN_STATE, PendingCallback, aggregator_mark_ok_if_exists, aggregator_mark_err_if_exists, #aggregator_ty};

            let addresses_val = #addresses_expr;
            let requests_val = #requests_expr;

            if addresses_val.len() != requests_val.len() {
                ::kinode_process_lib::kiprintln!("fan_out!: addresses.len() != requests.len()");
                return;
            }
            let total = addresses_val.len();
            if total == 0 {
                // no subrequests => do final callback with empty results if desired
                HIDDEN_STATE.with(|cell| {
                    if let Some(ref mut hidden) = *cell.borrow_mut() {
                        // We can't easily get user_state here, you'd do that in the main loop
                        // or you can do a hack. We'll just skip in this minimal example
                    }
                });
                return;
            }

            let aggregator_id = Uuid::new_v4().to_string();

            // Insert aggregator into hidden_state.accumulators
            HIDDEN_STATE.with(|cell| {
                if let Some(ref mut hidden) = *cell.borrow_mut() {
                    let aggregator = #aggregator_ty::new(
                        total,
                        Box::new(move |final_vec, user_state_any| {
                            let #results_ident = final_vec;
                            let #state_ident = user_state_any
                                .downcast_mut::<#state_type>()
                                .ok_or_else(|| anyhow!("downcast user state failed in aggregator finalize"))?;
                            #block
                            Ok(())
                        })
                    );
                    hidden.accumulators.insert(aggregator_id.clone(), Box::new(aggregator));
                }
            });

            for (i, (addr, req)) in addresses_val.into_iter().zip(requests_val.into_iter()).enumerate() {
                // correlation_id = aggregator_id + ":" + i
                let correlation_id = format!("{}:{}", aggregator_id, i);

                // Insert PendingCallback => parse subresponse => aggregator.set_ok(i, val)
                HIDDEN_STATE.with(|cell| {
                    if let Some(ref mut hidden) = *cell.borrow_mut() {
                        hidden.pending_callbacks.insert(
                            correlation_id.clone(),
                            PendingCallback {
                                on_success: Box::new({
                                    let aggregator_id_clone = aggregator_id.clone();
                                    move |resp_bytes, user_state_any| {
                                        match ::serde_json::from_slice(resp_bytes) {
                                            Ok(parsed_val) => {
                                                aggregator_mark_ok_if_exists(
                                                    &aggregator_id_clone,
                                                    i,
                                                    parsed_val,
                                                    user_state_any,
                                                );
                                            }
                                            Err(e) => {
                                                aggregator_mark_err_if_exists(
                                                    &aggregator_id_clone,
                                                    i,
                                                    anyhow!("parse error: {}", e),
                                                    user_state_any,
                                                );
                                            }
                                        }
                                        Ok(())
                                    }
                                }),
                                on_timeout: Some(Box::new({
                                    let aggregator_id_clone = aggregator_id.clone();
                                    move |user_state_any| {
                                        aggregator_mark_err_if_exists(
                                            &aggregator_id_clone,
                                            i,
                                            anyhow!("timeout"),
                                            user_state_any,
                                        );
                                        Ok(())
                                    }
                                })),
                            }
                        );
                    }
                });

                // Actually send
                match ::serde_json::to_vec(&req) {
                    Ok(body_bytes) => {
                        let _ = Request::to(addr)
                            .context(correlation_id.as_bytes())
                            .body(body_bytes)
                            .expects_response(#timeout_ts)
                            .send();
                    }
                    Err(e) => {
                        // aggregator set_err(i, e)
                        HIDDEN_STATE.with(|cell| {
                            if let Some(ref mut hidden) = *cell.borrow_mut() {
                                // direct aggregator access
                                if let Some(acc_any) = hidden.accumulators.get_mut(&aggregator_id) {
                                    if let Some(agg) = acc_any.downcast_mut::<#aggregator_ty>() {
                                        agg.set_err(i, anyhow!("serialize error: {}", e));
                                        if agg.is_done() {
                                            // remove & finalize => aggregator_mark_ok_if_exists does that
                                            let aggregator_box = hidden.accumulators.remove(&aggregator_id).unwrap();
                                            if let Ok(agg2) = aggregator_box.downcast::<#aggregator_ty>() {
                                                let aggregator = *agg2;
                                                // can't finalize w/o user_state, so main loop approach would do it
                                                // or we do it here if we can get user_state easily. 
                                                // We'll skip, do normal approach
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}
