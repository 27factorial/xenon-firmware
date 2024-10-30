use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn, Pat, ReturnType, Signature};

use crate::error::Errors;

macro_rules! to_compile_error {
    ($span:expr, $message:expr) => {
        syn::Error::new_spanned($span, $message).into_compile_error()
    };
}

pub fn expand(f: ItemFn) -> Result<TokenStream, TokenStream> {
    check_signature(&f.sig)?;

    let xenon_crate =
        match crate_name("xenon-firmware").expect("xenon-firmware to be in Cargo.toml") {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let ident = Ident::new(name.as_str(), Span::call_site());
                quote!(#ident)
            }
        };

    let wasmi_crate = match crate_name("wasmi").expect("wasmi to be in Cargo.toml") {
        FoundCrate::Itself => quote!(crate),
        FoundCrate::Name(name) => {
            let ident = Ident::new(name.as_str(), Span::call_site());
            quote!(#ident)
        }
    };

    let vis = f.vis;
    let name = f.sig.ident;
    let ret = f.sig.output;
    let body = f.block;
    let attrs = f.attrs;

    let mut inputs_iter = f.sig.inputs.iter();

    let first_input = inputs_iter.next().ok_or(to_compile_error!(
        &f.sig.inputs,
        "syscall function requires at least one argument (the caller)"
    ))?;

    let mut args: Vec<TokenStream> = Vec::new();
    let mut cvt_stmts: Vec<TokenStream> = Vec::new();

    for arg in inputs_iter {
        match arg {
            FnArg::Receiver(receiver) => {
                return Err(to_compile_error!(
                    receiver,
                    "syscall function must not have a receiver"
                ))
            }
            FnArg::Typed(pat_type) => {
                let Pat::Ident(ident) = &*pat_type.pat else {
                    return Err(to_compile_error!(
                        pat_type,
                        "only idents are supported for function parameters currently"
                    ));
                };

                let ty = &*pat_type.ty;

                let arg_tokens = quote_spanned! {
                    ident.span() =>
                    #ident: <#ty as #xenon_crate::app::convert::TryFromWasm>::WasmTy
                };

                let cvt_tokens = quote_spanned! {
                    ident.span() =>
                    let #ident = <#ty as #xenon_crate::app::convert::TryFromWasm>::try_from_wasm(#ident)
                        .map_err(|e| #xenon_crate::app::types::Error::InvalidValue(e.0))?;
                };

                args.push(arg_tokens);
                cvt_stmts.push(cvt_tokens);
            }
        }
    }

    let return_type = match &ret {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote_spanned!(ty.span() => #ty),
    };

    let verify_return_type = quote_spanned! {
        return_type.span() =>
        const _: () = {
            #[diagnostic::on_unimplemented(
                message = "`{Self}` is not a supported return type for syscall functions",
                label = "cannot use `{Self}` as a syscall return type"
            )]
            trait SyscallReturnType {}

            impl<T: #wasmi_crate::WasmRet> SyscallReturnType for T {}

            const fn __assert_valid_syscall_return_type<T>()
            where
                T: SyscallReturnType,
            {}

            #[allow(unused_parens)]
            __assert_valid_syscall_return_type::<(#return_type)>();
        };
    };

    Ok(quote! {
        #verify_return_type

        #(
            #attrs
        )*

        // Wasm only allows taking certain primitive types over FFI, so the number of arguments can
        // get quite large for more complex syscalls.
        #[allow(clippy::too_many_arguments)]
        #vis fn #name(
            #first_input,
            #(
                #args
            ),*
        ) #ret {
            #(
                #cvt_stmts
            )*

            {
                #body
            }
        }
    })
}

fn check_signature(sig: &Signature) -> Result<(), TokenStream> {
    let mut errors = Errors::new();

    if sig.constness.is_some() {
        errors.push(sig.constness, "syscall function must not be const")
    }

    if sig.asyncness.is_some() {
        errors.push(sig.asyncness, "syscall function must not be async")
    }

    if sig.unsafety.is_some() {
        errors.push(sig.unsafety, "syscall function must not be unsafe")
    }

    if !sig.generics.params.is_empty() {
        errors.push(&sig.generics.params, "syscall function must not be generic")
    }

    if sig.generics.where_clause.is_some() {
        errors.push(
            &sig.generics.where_clause,
            "syscall function must not have a `where` clause",
        )
    }

    let is_extern_wasm = sig
        .abi
        .as_ref()
        .and_then(|abi| abi.name.as_ref())
        .is_some_and(|s| s.value() == "wasm");

    if !is_extern_wasm {
        errors.push(sig, r#"syscall function must be an `extern "wasm"` fn"#);
    }

    if sig.variadic.is_some() {
        errors.push(&sig.variadic, "syscall function must not be variadic")
    }

    errors.check().map_err(Into::into)
}
