extern crate proc_macro;
use quote::{quote, ToTokens};
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

    // let cases: Vec<_> = fields.iter().filter_map(|x| {
    //     let field_const = x.ident.clone().map(|z| syn::Ident::new(&z.to_string().to_uppercase(), z.span()))?;
    //     let field_reader = match x.ty.clone() {
    //         e => panic!("Unsupported Parameter type {}", e.to_token_stream().to_string()),
    //     };

    //     Some(quote! { #field_const })
    // }).collect();

    let expanded = quote! {
        impl #name {
            #(pub const #field_names: &'static str = stringify!(#field_names); )*
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(ParameterField)]
pub fn derive_parameter_field(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // TODO: Safety for enum conversion
    let expanded = quote! {
        impl ParameterField for #name {
            fn from_data(data: &[u8]) -> Self {
                unsafe { std::mem::transmute::<u32, Self>(u32::from_data(data)) }
            }

            fn to_data(&self) -> Vec<u8> {
                (*self as u32).to_data()
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
