use std::fmt::Display;
use quote::{quote, ToTokens};
use syn::Error;

pub struct Errors(Vec<Error>);

impl Errors {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push<S, M>(&mut self, span: S, message: M)
    where
        S: ToTokens,
        M: Display,
    {
        self.0.push(Error::new_spanned(span, message));
    }

    pub fn check(&mut self) -> Result<(), proc_macro2::TokenStream> {
        match self.0.len() {
            0 => Ok(()),
            _ => {
                let errors = self.0.drain(..).map(Error::into_compile_error);

                Err(quote! {
                    #(#errors)*
                })
            }
        }
    }
}
