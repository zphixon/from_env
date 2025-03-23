# from_env

Define a configuration structure which implements `Deserialize`, and whose
values can be overridden by setting environment variables.

For example:

```rust
// Defines a struct `Config`
from_env::config!(
    // Define a base type name, this will the the environment variable namespace
    "TestStuff",

    // Define some config values. These must implement FromStr and Deserialize
    hello: String,
    network {
        // Mappings are also possible
        address: SocketAddr,
        database_url: Url,
    }
);
```

```toml
hello = "world"

[network]
address = "0.0.0.0:5000"
database_url = "mysql://root:password@localhost:3306/database"
```

```json
{
    "hello": "world",
    "network": {
        "address": "0.0.0.0:5000",
        "database_url": "mysql://root:password@localhost:3306/database"
    }
}
```

```rust
let mut config: Config = toml::from_str(config_contents).unwrap();
let mut config: Config = serde_json::from_str(config_contents).unwrap();

// Call hydrate_from_env to override with env vars
config.hydrate_from_env();
```
