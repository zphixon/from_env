use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{parse::Parse, punctuated::Punctuated, Attribute, Ident, LitStr, Meta, Token, Type};

#[derive(Clone)]
enum ConfigValue {
    Leaf(Type),
    Table(Vec<ConfigOption>),
}

#[derive(Clone)]
struct ConfigOption {
    attrs: Option<Vec<Attribute>>,
    is_default: bool,
    name: Ident,
    value: Box<ConfigValue>,
}

impl Parse for ConfigOption {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        let attrs = if lookahead.peek(Token![#]) {
            Some(Attribute::parse_outer(input)?)
        } else {
            None
        };

        // check if #[derive(Default)]
        let derive_path: syn::Path = syn::parse_quote! { derive };
        let default_path: syn::Path = syn::parse_quote! { Default };
        let is_default = if let Some(attrs) = attrs.as_ref() {
            attrs.iter().any(|attr| match &attr.meta {
                Meta::List(meta_list) if meta_list.path == derive_path => {
                    let derived_traits = meta_list
                        .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                        .unwrap();
                    derived_traits
                        .iter()
                        .any(|derived_trait| derived_trait == &default_path)
                }
                _ => false,
            })
        } else {
            false
        };

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

        Ok(ConfigOption {
            attrs,
            is_default,
            name,
            value,
        })
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

        let attrs = self.attrs.clone().unwrap_or_default();
        let serde_default = if self.is_default {
            quote! { #[serde(default)] }
        } else {
            quote! {}
        };

        match &*self.value {
            ConfigValue::Leaf(type_) => quote! {
                #(#attrs)*
                #serde_default
                #name : #type_,
            },
            ConfigValue::Table(_) => quote! {
                #serde_default
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

                let attrs = self.attrs.clone().unwrap_or_default();

                quote! {
                    #[derive(serde::Deserialize, Debug)]
                    #[allow(non_camel_case_types)]
                    #(#attrs)*
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
                        use serde::Deserialize;
                        // idk how to indent this
                        self.#field = <#leaf>::deserialize(de::StringDeserializer {
                            input: value.as_str()
                        }).expect(concat!("invalid value for ", #name));
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
            options: input
                .parse_terminated(ConfigOption::parse, Token![,])?
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

        mod de {
            use serde::{de::{IntoDeserializer, Visitor},};

            #[derive(Debug, thiserror::Error)]
            pub(super) enum StringDeserializerError<'de> {
                #[error("Custom error: {0}")]
                Custom(String),

                #[error("Could not parse bool: {0}")]
                ParseBool(#[from] std::str::ParseBoolError),

                #[error("Could not parse int: {0}")]
                ParseInt(#[from] std::num::ParseIntError),

                #[error("Could not parse float: {0}")]
                ParseFloat(#[from] std::num::ParseFloatError),

                #[error("Cannot deseralize {0}")]
                CannotDeserialize(&'static str),

                #[error("Zero or more than one char: {0:?}")]
                NotChar(&'de str),

                #[error("Not unit struct {0}: {1:?}")]
                NotUnit(&'static str, &'de str),

                #[error("Enum variant does not exist: {0:?}")]
                NoSuchVariant(&'de str),

                #[error("The string deserializer is not self-describing")]
                NotSelfDescribing,
            }

            impl<'de> serde::de::Error for StringDeserializerError<'de> {
                fn custom<T>(msg: T) -> Self
                where
                    T: std::fmt::Display,
                {
                    StringDeserializerError::Custom(msg.to_string())
                }
            }

            #[derive(Clone, Copy)]
            pub(super) struct StringDeserializer<'de> {
                pub(super) input: &'de str,
            }

            impl<'de> serde::de::Deserializer<'de> for StringDeserializer<'de> {
                type Error = StringDeserializerError<'de>;

                fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    Err(StringDeserializerError::NotSelfDescribing)
                }

                fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_bool(self.input.parse()?)
                }

                fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_i8(self.input.parse()?)
                }

                fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_i16(self.input.parse()?)
                }

                fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_i32(self.input.parse()?)
                }

                fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_i64(self.input.parse()?)
                }

                fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_u8(self.input.parse()?)
                }

                fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_u16(self.input.parse()?)
                }

                fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_u32(self.input.parse()?)
                }

                fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_u64(self.input.parse()?)
                }

                fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_f32(self.input.parse()?)
                }

                fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_f64(self.input.parse()?)
                }

                fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    if self.input.len() != 1 {
                        return Err(StringDeserializerError::NotChar(self.input));
                    }
                    visitor.visit_char(self.input.chars().nth(0).unwrap())
                }

                fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_str(self.input)
                }

                fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_string(String::from(self.input))
                }

                fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_bytes(self.input.as_bytes())
                }

                fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_byte_buf(self.input.as_bytes().to_owned())
                }

                fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    if self.input == "" {
                        visitor.visit_none()
                    } else {
                        visitor.visit_some(self)
                    }
                }

                fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    self.deserialize_unit_struct("", visitor)
                }

                fn deserialize_unit_struct<V>(
                    self,
                    name: &'static str,
                    visitor: V,
                ) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    if self.input == name {
                        visitor.visit_unit()
                    } else {
                        Err(StringDeserializerError::NotUnit(name, self.input))
                    }
                }

                fn deserialize_newtype_struct<V>(
                    self,
                    _name: &'static str,
                    visitor: V,
                ) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_newtype_struct(self)
                }

                fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_seq(CommaSeparated::new(self))
                }

                fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_seq(CommaSeparated::new(self))
                }

                fn deserialize_tuple_struct<V>(
                    self,
                    _name: &'static str,
                    _len: usize,
                    visitor: V,
                ) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    visitor.visit_seq(CommaSeparated::new(self))
                }

                fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    Err(StringDeserializerError::CannotDeserialize("maps"))
                }

                fn deserialize_struct<V>(
                    self,
                    _name: &'static str,
                    _fields: &'static [&'static str],
                    _visitor: V,
                ) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    Err(StringDeserializerError::CannotDeserialize("structs"))
                }

                fn deserialize_enum<V>(
                    self,
                    _name: &'static str,
                    variants: &'static [&'static str],
                    visitor: V,
                ) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    if variants.contains(&self.input) {
                        visitor.visit_enum(self.input.into_deserializer())
                    } else {
                        Err(StringDeserializerError::NoSuchVariant(self.input))
                    }
                }

                fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    self.deserialize_str(visitor)
                }

                fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where
                    V: Visitor<'de>,
                {
                    self.deserialize_any(visitor)
                }
            }

            struct CommaSeparated<'de> {
                de: StringDeserializer<'de>,
                bytes: std::iter::Peekable<std::iter::Enumerate<std::str::Bytes<'de>>>,
            }

            impl<'de> CommaSeparated<'de> {
                fn new(de: StringDeserializer<'de>) -> Self {
                    CommaSeparated {
                        bytes: de.input.bytes().enumerate().peekable(),
                        de,
                    }
                }
            }

            impl<'de> serde::de::SeqAccess<'de> for CommaSeparated<'de> {
                type Error = StringDeserializerError<'de>;

                fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
                where
                    T: serde::de::DeserializeSeed<'de>,
                {
                    let Some(&(start, _)) = self.bytes.peek() else {
                        return Ok(None);
                    };
                    while {
                        if let Some((_, byte)) = self.bytes.next() {
                            byte != b','
                        } else {
                            false
                        }
                    } {}
                    let end = match self.bytes.peek() {
                        Some(&(end, _)) => end - 1,
                        None => self.de.input.len(),
                    };

                    seed.deserialize(StringDeserializer {
                        input: &self.de.input[start..end],
                    })
                    .map(Some)
                }
            }
        }
    }
    .into_token_stream()
    .into()
}
