use proc_macro::TokenStream;
use syn::parse_macro_input;
use quote::quote;

#[proc_macro]
pub fn hyperprocess_(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::Item);
    // For now, just echo the input back as a sanity check
    quote!(#input).into()
}
