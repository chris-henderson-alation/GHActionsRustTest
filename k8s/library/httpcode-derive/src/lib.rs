extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Fields};

// https://blog.turbo.fish/proc-macro-simple-derive/

#[proc_macro_derive(HttpCode, attributes(code))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    match input.data {
        Data::Struct(DataStruct { .. }) => {
            let code: Option<&Attribute> =
                input.attrs.iter().find(|attr| attr.path.is_ident("code"));
            match code {
                Some(attribute) => {
                    let tt: Expr = attribute.parse_args().unwrap();
                    quote!(
                        impl HttpCode for #name {
                            fn http_code(&self) -> httpcode::Status {
                                #tt
                            }
                        }
                    )
                    .into()
                }
                None => panic!("struct must have #[code(<CODE>)] attribute"),
            }
        }
        Data::Enum(DataEnum { variants, .. }) => {
            let q = variants.iter().map(|variant| {
                let v = &variant.ident;
                let code: Option<&Attribute> =
                    variant.attrs.iter().find(|attr| attr.path.is_ident("code"));
                if code.is_none() {
                    panic!("variant {} missigin code attribute", v);
                }
                let code: Expr = code.unwrap().parse_args().unwrap();
                match variant.fields {
                    Fields::Unnamed(_) => quote! {
                        #name::#v(..) => { #code }
                    },
                    Fields::Named(_) => quote! {
                        #name::#v{ .. } => { #code }
                    },
                    Fields::Unit => quote! {
                        #name::#v => { #code }
                    },
                }
            });
            quote!(
                impl HttpCode for #name {
                    fn http_code(&self) -> httpcode::Status {
                        match self {
                            #(#q),*
                        }
                    }
                }
            )
            .into()
        }
        Data::Union(..) => panic!("just say no to unions"),
    }
}
