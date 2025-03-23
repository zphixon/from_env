use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{parse::Parse, punctuated::Punctuated, Ident, LitStr, Token, Type};

#[derive(Clone)]
enum ConfigValue {
    Leaf(Type),
    Table(Vec<ConfigOption>),
}

#[derive(Clone)]
struct ConfigOption {
    name: Ident,
    value: Box<ConfigValue>,
}

impl Parse for ConfigOption {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;

        let lookahead = input.lookahead1();
        let value = Box::new(if lookahead.peek(Token![:]) {
            let _colon: Token![:] = input.parse()?;
            let type_: Type = input.parse()?;
            ConfigValue::Leaf(type_)
        } else {
            let content;
            syn::braced!(content in input);
            let options = content
                .parse_terminated(ConfigOption::parse, Token![,])?
                .iter()
                .cloned()
                .collect();
            ConfigValue::Table(options)
        });

        Ok(ConfigOption { name, value })
    }
}

impl ConfigOption {
    fn type_name(&self) -> Option<Ident> {
        match &*self.value {
            ConfigValue::Leaf(_) => None,
            ConfigValue::Table(_) => Some(Ident::new(
                &(self.name.to_string() + "Config"),
                self.name.span(),
            )),
        }
    }

    fn as_struct_field(&self) -> TokenStream2 {
        let name = self.name.clone();
        let newtypename = self.type_name();
        match &*self.value {
            ConfigValue::Leaf(type_) => quote! {
                #name : #type_ ,
            },
            ConfigValue::Table(_) => quote! {
                #name : #newtypename,
            },
        }
    }

    fn as_type(&self, parent_name: String) -> TokenStream2 {
        let newtypename = self.type_name();
        match &*self.value {
            ConfigValue::Leaf(_) => quote! {},
            ConfigValue::Table(children) => {
                let fields = children
                    .iter()
                    .map(|child| child.as_struct_field())
                    .collect::<Vec<_>>();

                let new_parent = parent_name + "_" + &self.name.to_string().to_uppercase();

                let hydrate_fields = children
                    .iter()
                    .map(|child| child.hydrate_fields(new_parent.clone()))
                    .collect::<Vec<_>>();

                let types = children
                    .iter()
                    .map(|child| child.as_type(new_parent.clone()))
                    .collect::<Vec<_>>();

                quote! {
                    #[derive(serde::Deserialize, Debug)]
                    #[allow(non_camel_case_types)]
                    struct #newtypename {
                        #(#fields)*
                    }
                    #(#types)*
                    impl #newtypename {
                        fn hydrate_from_env(&mut self) {
                            #(#hydrate_fields)*
                        }
                    }
                }
            }
        }
    }

    fn hydrate_fields(&self, parent_name: String) -> TokenStream2 {
        let field = self.name.clone();
        let name = parent_name + "_" + &self.name.to_string().to_uppercase();
        match &*self.value {
            ConfigValue::Leaf(leaf) => quote! {
                match std::env::var(#name) {
                    Ok(value) => {
                        use std::str::FromStr;
                        self.#field = <#leaf>::from_str(value.as_str()).expect(
                            concat!("invalid value for ", #name)
                        );
                    }
                    _ => {}
                }
            },
            ConfigValue::Table(_) => {
                let name = self.name.clone();
                quote! {
                    self.#name.hydrate_from_env();
                }
            }
        }
    }
}

struct ConfigCall {
    project_name: String,
    options: Vec<ConfigOption>,
}

mod kws {
    syn::custom_keyword!(projectname);
    syn::custom_keyword!(options);
}

impl Parse for ConfigCall {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let project_name: LitStr = input.parse()?;
        let _: Token![,] = input.parse()?;
        Ok(ConfigCall {
            project_name: project_name.value(),
            options: Punctuated::<ConfigOption, Token![,]>::parse_separated_nonempty(input)?
                .iter()
                .cloned()
                .collect(),
        })
    }
}

#[proc_macro]
pub fn config(tokens: TokenStream) -> TokenStream {
    let config: ConfigCall = syn::parse_macro_input!(tokens);

    let fields = config
        .options
        .iter()
        .map(|option| option.as_struct_field())
        .collect::<Vec<_>>();

    let hydrate_fields = config
        .options
        .iter()
        .map(|option| option.hydrate_fields(config.project_name.to_uppercase()))
        .collect::<Vec<_>>();

    let types = config
        .options
        .iter()
        .map(|option| option.as_type(config.project_name.to_uppercase()))
        .collect::<Vec<_>>();

    quote! {
        #[derive(serde::Deserialize, Debug)]
        struct Config {
            #(#fields)*
        }

        #(#types)*

        impl Config {
            fn hydrate_from_env(&mut self) {
                #(#hydrate_fields)*
            }
        }
    }
    .into_token_stream()
    .into()
}
