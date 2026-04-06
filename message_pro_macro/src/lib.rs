extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Meta, Lit, Expr,
    punctuated::Punctuated,
    Token,
};

#[proc_macro_derive(ConnectionMessage, attributes(connection_message))]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = ast.ident;

    let mut is_authentication = false;

    for attr in ast.attrs {
        if attr.path().is_ident("connection_message") {
            let list = attr.parse_args_with(
                Punctuated::<Meta, Token![,]>::parse_terminated
            );

            if let Ok(list) = list {
                for meta in list {
                    if let Meta::NameValue(nv) = meta && nv.path.is_ident("authentication") && let Expr::Lit(expr_lit) = nv.value && let Lit::Bool(b) = expr_lit.lit {
                        is_authentication = b.value;
                    }
                }
            }
        }
    }

    let expanded = if is_authentication {
        quote! {
            impl MessageTrait for #name {
                fn as_authentication(&self) -> bool { true }

                fn deserialize(data: &[u8]) -> Self {
                    postcard::from_bytes(data).unwrap()
                }
            }
        }
    } else {
        quote! {
            impl MessageTrait for #name {
                fn as_authentication(&self) -> bool { false }

                fn deserialize(data: &[u8]) -> Self {
                    postcard::from_bytes(data).unwrap()
                }
            }
        }
    };

    expanded.into()
}
