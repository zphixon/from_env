use std::net::SocketAddr;
use url::Url;

from_env::config!(
    "SimpleExample",

    hello: String,
    network {
        address: SocketAddr,
        database_url: Url,
    },
    world {
        bungle: Option<String>,
        wungle {
            fungle: Vec<String>,
        },
    }
);

fn main() {
    let mut config: Config = toml::from_str(
        r#"

hello = "is anybody out there"

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
