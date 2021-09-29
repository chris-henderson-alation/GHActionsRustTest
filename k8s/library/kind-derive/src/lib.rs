extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DataEnum, DataStruct, DataUnion, DeriveInput, Fields};

#[proc_macro_derive(Kind)]
pub fn kind(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    match input.data {
        Data::Struct(DataStruct{..}) => quote!(
            impl Kind for #name {
                fn kind(&self) -> String {
                    stringify!(#name).to_string()
                }
            }
        ),
        Data::Enum(DataEnum{ variants, ..  }) => {
            let q = variants.iter().map(|variant| {
                let v = &variant.ident;
                match variant.fields {
                    Fields::Unnamed(_) => quote! {
                        #name::#v(..) => concat!(stringify!(#name), stringify!(::), stringify!(#v)).to_string()
                    },
                    Fields::Named(_) => quote!{
                        #name::#v{ .. } => concat!(stringify!(#name), stringify!(::), stringify!(#v)).to_string()
                    },
                    Fields::Unit => quote!{
                        #name::#v => concat!(stringify!(#name), stringify!(::), stringify!(#v)).to_string()
                    }
                }
            });
            quote!(
                impl Kind for #name {
                    fn kind(&self) -> String {
                        match self {
                            #(#q),*
                        }
                    }
                }
            )
        }
        Data::Union(DataUnion {  .. }) => {
            // Sorry, unions are more for either FFI with C code
            // or for embedded devices and that's just not our use case.
            //
            // At any rate, at least this is what the compiler error
            // will look like which lets the user know how to proceed
            // forward if they stil want this.
            //
            // error: proc-macro derive panicked
            //   --> src/mod:84:18
            //    |
            // 84 |         #[derive(Kind)]
            //    |                  ^^^^
            //    |
            //    = help: message: kind-derive does not support Unions yet. Perhaps you should try manually implementing Kind?
            //
            //            r#"impl Kind for MyUnion {
            //                fn kind(&self) -> &'static str {
            //                    ...
            //                }
            //            }
            panic!(r#"kind-derive does not support Unions yet. Perhaps you should try manually implementing Kind?

r#"impl Kind for {} {{
    fn kind(&self) -> &'static str {{
        ...
    }}
}}"#, name)
        }
    }.into()
}
