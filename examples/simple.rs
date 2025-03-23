use std::{net::SocketAddr, str::FromStr};
use serde::{de::Visitor, Deserialize};
use url::Url;

from_env::config!(
    "SimpleExample",

    hello: String,
    network {
        address: SocketAddr,
        database_url: Url,
    },
    world {
        bungle: String,
        wungle {
            fungle: StringList,
        },
    }
);

#[derive(Debug)]
struct StringList {
    list: Vec<String>,
}
impl FromStr for StringList {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StringList {
            list: s.split(",").map(String::from).collect(),
        })
    }
}
impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor {}
        impl<'de> Visitor<'de> for SeqVisitor {
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut list = Vec::new();
                while let Some(next) = seq.next_element()? {
                    list.push(next);
                }
                Ok(list)
            }
            type Value = Vec<String>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "string")
            }
        }
        Ok(StringList {
            list: deserializer.deserialize_seq(SeqVisitor {})?,
        })
    }
}

fn main() {
    let mut config: Config = toml::from_str(
        r#"

hello = "is anybody out there"

[network]
address = "192.168.56.10:7887"
database_url = "https://blueberry"

[world]
bungle = "fungle"
wungle.fungle = ["1", "2", "3"]

"#,
    )
    .unwrap();

    config.hydrate_from_env();

    println!("{:#?}", config);
}
