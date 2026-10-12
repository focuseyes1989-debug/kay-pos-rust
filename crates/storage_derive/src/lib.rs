use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(FromRow, attributes(sqlx))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let Data::Struct(data) = input.data else {
        panic!("FromRow requires a struct")
    };
    let Fields::Named(fields) = data.fields else {
        panic!("FromRow requires named fields")
    };
    let fields = fields.named.iter().map(|field| {
        let name = field.ident.as_ref().unwrap();
        let skipped = field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("sqlx") && quote!(#attr).to_string().contains("skip"));
        if skipped {
            quote!(#name: Default::default())
        } else {
            let column = name.to_string();
            quote!(#name: row.decode_named(#column)?)
        }
    });
    quote! { impl ::pos_core::storage::FromRow for #name {
        fn from_row(row: &::pos_core::storage::Row) -> Result<Self, ::sqlx::Error> {
            Ok(Self { #(#fields),* })
        }
    }}
    .into()
}
