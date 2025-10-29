# backpackrust

## Build prerequisites (Linux)

This project currently uses TLS via the system OpenSSL through crates like `native-tls`.
On Debian/Ubuntu, install the OpenSSL development headers and pkg-config before building:

```
sudo apt-get update
sudo apt-get install -y pkg-config libssl-dev
```

Then build with Cargo:

```
cargo build
```

### Alternative: use rustls (no system OpenSSL)

If you prefer not to install system OpenSSL, you can switch dependencies to `rustls`:

- For `reqwest`, disable default features and enable `rustls-tls`.
- For `tokio-tungstenite`, use the `rustls-tls-native-roots` (or `rustls-tls-webpki-roots`) feature instead of `native-tls`.

Example `Cargo.toml` changes:

```
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio-tungstenite = { version = "0.23", features = ["rustls-tls-native-roots"] }
```

This removes the OpenSSL dependency at build time.
