use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn attr(_args: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[test]
fn test() {}
