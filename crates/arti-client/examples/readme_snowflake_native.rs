//! Native Snowflake Example: Full arti-client bootstrap via Snowflake
//!
//! This example connects to the Tor network via Snowflake WebSocket bridge
//! using native (non-WASM) code and performs a full directory bootstrap.
//! This helps isolate whether directory download issues are WASM-specific
//! or inherent to the Snowflake transport.
//!
//! # Building and Running
//!
//! ```sh
//! RUST_LOG=info cargo run --example readme_snowflake_native -p arti-client --features experimental-api
//! ```

// Lint configuration for examples
#![allow(clippy::print_stdout)]
#![allow(clippy::unwrap_used)]

use anyhow::Result;
use std::sync::Arc;
use tokio_crate as tokio;
use tracing::info;

use arti_client::{TorClient, TorClientConfig};
use arti_client::config::{BridgeConfigBuilder, CfgPath, pt::TransportConfigBuilder};
use futures::io::{AsyncReadExt, AsyncWriteExt};
use tor_rtcompat::tokio::TokioNativeTlsRuntime;
use webtor_rs::arti_transport_native::SnowflakePtMgr;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    tracing_subscriber::fmt::init();

    info!("=== Native Snowflake Full Bootstrap Test ===");
    info!("This will bootstrap arti-client through Snowflake bridge");
    info!("Using PSE Snowflake bridge (accepts non-browser clients)");
    info!("");

    // Create runtime
    let runtime = TokioNativeTlsRuntime::current()?;

    // Create Snowflake PT Manager
    let snowflake_mgr = SnowflakePtMgr::new(runtime.clone());
    info!("Created Snowflake PT manager");

    // Configure arti-client with Snowflake bridge
    let mut config_builder = TorClientConfig::builder();

    // Use default storage paths
    config_builder
        .storage()
        .cache_dir(CfgPath::new("/tmp/arti-snowflake-test/cache".to_owned()))
        .state_dir(CfgPath::new("/tmp/arti-snowflake-test/state".to_owned()));

    // Configure the Snowflake bridge
    // Format: "snowflake <dummy-addr> <fingerprint>"
    const SNOWFLAKE_BRIDGE_LINE: &str =
        "snowflake 0.0.2.0:1 664A92FF3EF71E03A2F09B1DAABA2DDF920D5194";

    let bridge: BridgeConfigBuilder = SNOWFLAKE_BRIDGE_LINE.parse()?;
    config_builder.bridges().bridges().push(bridge);

    // Add a transport config entry for "snowflake"
    let mut transport = TransportConfigBuilder::default();
    transport
        .protocols(vec!["snowflake".parse()?])
        .proxy_addr("127.0.0.1:1".parse()?);
    config_builder.bridges().transports().push(transport);

    let config = config_builder.build()?;
    info!("Configuration built with Snowflake bridge");

    // Create TorClient without bootstrapping so we can inject our PT manager
    let tor_client = TorClient::with_runtime(runtime)
        .config(config)
        .create_unbootstrapped()?;

    info!("TorClient created (unbootstrapped)");

    // Inject our Snowflake PT manager
    #[cfg(feature = "experimental-api")]
    {
        tor_client.chanmgr().set_pt_mgr(Arc::new(snowflake_mgr));
        info!("Snowflake PT manager injected into ChanMgr");
    }

    #[cfg(not(feature = "experimental-api"))]
    {
        eprintln!("ERROR: experimental-api feature not enabled!");
        eprintln!("Run with: --features experimental-api");
        return Ok(());
    }

    // Bootstrap the client
    info!("");
    info!("=== Starting Bootstrap ===");
    info!("This will download directory data through Snowflake...");
    info!("");

    tor_client.bootstrap().await?;
    info!("Bootstrap complete!");

    // Make a connection through Tor to verify it works
    info!("");
    info!("=== Testing Connection ===");
    info!("Connecting to example.com:80...");

    let mut stream = tor_client.connect(("example.com", 80)).await?;

    info!("Sending HTTP request...");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
        .await?;
    stream.flush().await?;

    info!("Reading response...");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    let response = String::from_utf8_lossy(&buf);
    info!("Response received ({} bytes):", buf.len());
    println!("{}", &response[..response.len().min(500)]);

    info!("");
    info!("=== SUCCESS ===");
    info!("Native Snowflake bootstrap and connection completed!");

    Ok(())
}