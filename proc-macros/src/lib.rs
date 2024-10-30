mod error;
mod expand;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn syscall(_: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);

    match expand::expand(f) {
        Ok(tokens) => tokens.into(),
        Err(tokens) => tokens.into(),
    }
}
