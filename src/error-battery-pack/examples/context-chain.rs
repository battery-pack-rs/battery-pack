//! Error context chains with anyhow.
//!
//! Run with: `cargo run --example context-chain -p error-battery-pack`
//!
//! Each layer in the call stack adds context, producing output like:
//!
//! ```text
//! failed to start server
//!
//! Caused by:
//!     0: failed to load config from "server.toml"
//!     1: invalid value for field "port"
//!     2: invalid digit found in string
//! ```

use anyhow::{Context, Result, bail};

fn parse_field(value: &str) -> Result<u16> {
    let port: u16 = value.trim().parse()?;

    if port == 0 {
        bail!("port must not be zero");
    }

    Ok(port)
}

fn load_config(_path: &str) -> Result<u16> {
    // Simulate reading a config file with a bad port value
    let raw_config = "port = notanumber";

    let port_str = raw_config
        .strip_prefix("port = ")
        .context("missing 'port' field")?;

    parse_field(port_str).context("invalid value for field \"port\"")
}

fn start_server() -> Result<()> {
    let path = "server.toml";
    let port =
        load_config(path).with_context(|| format!("failed to load config from \"{path}\""))?;

    println!("listening on port {port}");
    Ok(())
}

fn main() {
    if let Err(e) = start_server().context("failed to start server") {
        // The {:#} formatter shows the full chain on one line:
        //   failed to start server: failed to load config from "server.toml": ...
        println!("One-line:   {e:#}");

        // The {:?} formatter shows the chain vertically:
        //   failed to start server
        //   Caused by:
        //       0: failed to load config...
        //       1: invalid value for field "port"
        //       2: invalid digit found in string
        println!("\nFull chain:\n{e:?}");
    }
}
