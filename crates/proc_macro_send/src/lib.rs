use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::Parse, parse::ParseStream, parse_macro_input, Block, Expr, Expr::Call as ExprCallNode,
    Expr::Path as ExprPathNode, ExprCall, ExprPath, Ident, Result, Token, Type,
};

// The main macro entry point
#[proc_macro]
pub fn send_async(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as SendAsyncInvocation);
    invocation.expand_macro().into()
}

/// Our data structure representing the macro invocation
struct SendAsyncInvocation {
    destination: Expr,
    request_expr: Expr,

    resp_ident: Ident,
    st_ident: Ident,
    st_type: Type,
    callback_block: Block,

    timeout: Option<Expr>,
    on_timeout_block: Option<Block>,
}

impl Parse for SendAsyncInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        // 1) destination expr
        let destination: Expr = input.parse()?;
        input.parse::<Token![,]>()?;

        // 2) request expr
        let request_expr: Expr = input.parse()?;
        input.parse::<Token![,]>()?;

        // 3) parse `(resp, st: MyState) { ... }`
        let content;
        syn::parenthesized!(content in input);
        let resp_ident: Ident = content.parse()?;
        content.parse::<Token![,]>()?;
        let st_ident: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let st_type: Type = content.parse()?;

        let callback_block: Block = input.parse()?;

        // optional trailing
        let mut timeout: Option<Expr> = None;
        let mut on_timeout_block: Option<Block> = None;

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            // Could be an expr for the timeout or "on_timeout => ..."
            if input.peek(syn::LitInt) || input.peek(syn::Ident) || input.peek(syn::Lit) {
                timeout = Some(input.parse()?);

                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                    if input.peek(Ident) {
                        let on_timeout_ident: Ident = input.parse()?;
                        if on_timeout_ident == "on_timeout" {
                            input.parse::<Token![=>]>()?;
                            let block: Block = input.parse()?;
                            on_timeout_block = Some(block);
                        }
                    }
                }
            } else if input.peek(Ident) {
                let on_timeout_ident: Ident = input.parse()?;
                if on_timeout_ident == "on_timeout" {
                    input.parse::<Token![=>]>()?;
                    let block: Block = input.parse()?;
                    on_timeout_block = Some(block);
                }
            }
        }

        Ok(Self {
            destination,
            request_expr,
            resp_ident,
            st_ident,
            st_type,
            callback_block,
            timeout,
            on_timeout_block,
        })
    }
}

impl SendAsyncInvocation {
    fn expand_macro(&self) -> proc_macro2::TokenStream {
        // 1) variant name from request
        let variant_ident = extract_variant_name(&self.request_expr)
            .unwrap_or_else(|| syn::Ident::new("UNKNOWN_VARIANT", proc_macro2::Span::call_site()));

        // 2) build "response path" => rename SomethingRequest -> SomethingResponse
        let response_path = build_response_path(&self.request_expr);

        // 3) We'll match on response_path::Variant(inner) => inner
        let success_arm = quote! {
            #response_path :: #variant_ident(inner) => inner
        };

        // 4) default timeout is 30
        let timeout_expr = self
            .timeout
            .clone()
            .unwrap_or_else(|| syn::parse_str("30").unwrap());

        // 5) optional on_timeout code
        let on_timeout_code = if let Some(ref block) = self.on_timeout_block {
            let st_ident = &self.st_ident;
            let st_type = &self.st_type;
            quote! {
                Some(Box::new(move |any_state: &mut dyn std::any::Any| {
                    let #st_ident = any_state
                        .downcast_mut::<#st_type>()
                        .ok_or_else(|| anyhow::anyhow!("Downcast to user state failed!"))?;
                    #block
                    Ok(())
                }))
            }
        } else {
            quote! { None }
        };

        let dest_expr = &self.destination;
        let request_expr = &self.request_expr;
        let resp_ident = &self.resp_ident;
        let st_ident = &self.st_ident;
        let st_type = &self.st_type;
        let callback_body = &self.callback_block;

        // 6) produce final expansion
        quote! {
            {

                // Serialize the request
                match ::serde_json::to_vec(&#request_expr) {
                    Ok(b) => {
                        let correlation_id = ::uuid::Uuid::new_v4().to_string();

                        // Insert callback into your global
                        {
                            // e.g. if your global is in kinode_app_common:
                            let mut guard = kinode_app_common::GLOBAL_APP_STATE.lock().unwrap();
                            if let Some(app_state) = guard.as_mut() {
                                app_state.pending_callbacks.insert(
                                    correlation_id.clone(),
                                    kinode_app_common::PendingCallback {
                                        on_success: Box::new(move |resp_bytes: &[u8], any_state: &mut dyn std::any::Any| {
                                            // parse entire <response_path> enum
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

                                            #callback_body
                                            Ok(())
                                        }),
                                        on_timeout: #on_timeout_code,
                                    }
                                );
                            }
                        }
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
            }
        }
    }
}

/// Extract the final variant name from e.g. MyCoolRequest::Foo(...).
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

/// Build a "response path" by rewriting XyzRequest -> XyzResponse.
fn build_response_path(expr: &Expr) -> proc_macro2::TokenStream {
    if let ExprCallNode(ExprCall { func, .. }) = expr {
        if let ExprPathNode(ExprPath { path, .. }) = &**func {
            let segments = &path.segments;
            if segments.len() < 2 {
                // can't rewrite if there's no last enum seg
                return quote! { UNKNOWN_RESPONSE };
            }

            // Get the enum name segment (second to last) and the variant (last)
            let enum_seg = &segments[segments.len() - 2];

            // Create the new response enum name
            let old_ident_str = enum_seg.ident.to_string();
            let new_ident_str = if let Some(base) = old_ident_str.strip_suffix("Request") {
                format!("{}Response", base)
            } else {
                format!("{}Response", old_ident_str)
            };
            let new_ident = syn::Ident::new(&new_ident_str, enum_seg.ident.span());

            // Reconstruct the path with the new response enum name
            let mut new_segments = syn::punctuated::Punctuated::new();

            // Add all segments except the last two
            for i in 0..segments.len() - 2 {
                new_segments.push(segments[i].clone());
            }

            // Add our new response enum segment
            let mut replaced_seg = enum_seg.clone();
            replaced_seg.ident = new_ident;
            new_segments.push(replaced_seg);

            // Create the new path
            let new_path = syn::Path {
                leading_colon: path.leading_colon,
                segments: new_segments,
            };
            return quote! { #new_path };
        }
    }
    // fallback
    quote! { UNKNOWN_RESPONSE }
}
