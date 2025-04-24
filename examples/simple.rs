use serde::Deserialize;
use std::net::SocketAddr;
use url::Url;

from_env::config!(
    "EXAMPLE",

    #[serde(default = "default_hello")]
    hello: String,
    network {
        address: SocketAddr,
        database_url: Url,
    },
    #[derive(Default)]
    world {
        bungle: Option<String>,
        #[derive(Default)]
        wungle {
            #[serde(default)]
            fungle: Vec<String>,
        },
    },
);

fn default_hello() -> String {
    String::from("is anybody out there")
}

fn main() {
    let mut config: Config = toml::from_str(
        r#"
[network]
address = "192.168.56.10:7887"
database_url = "https://blueberry"

[world]
wungle.fungle = ["1", "2", "3"]

"#,
    )
    .unwrap();

    config.hydrate_from_env();

    println!("{:#?}", config);
}
