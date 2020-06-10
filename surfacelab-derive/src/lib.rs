extern crate proc_macro;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Parameters)]
pub fn derive_parameters(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = match input.data {
        syn::Data::Struct(dstruct) => dstruct.fields,
        _ => panic!("Expected struct for Parameters derivation"),
    };

    let field_names = fields.iter().filter_map(|x| {
        x.ident
            .clone()
            .map(|z| syn::Ident::new(&z.to_string().to_uppercase(), z.span()))
    });

    let expanded = quote! {
        impl #name {
            #(pub const #field_names: &'static str = stringify!(#field_names); )*
        }
    };

    proc_macro::TokenStream::from(expanded)
}
