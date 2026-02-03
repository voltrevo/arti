//! WASM Example: Using arti-client with Snowflake transport
//!
//! This example demonstrates how to use arti-client in a WASM environment
//! with Snowflake bridges for censorship circumvention.
//!
//! # Building for WASM
//!
//! ```sh
//! cargo build --example wasm_snowflake --target wasm32-unknown-unknown \
//!     --no-default-features --features pt-client,experimental-api
//! ```
//!
//! # Note
//!
//! This example requires:
//! - The `webtor-rs` crate with `arti-integration` feature
//! - The `experimental-api` feature to access chanmgr for PT injection
//! - WASM-compatible runtime (automatically selected)

// Lint configuration for examples
#![allow(clippy::print_stdout)]
#![allow(clippy::unwrap_used)]

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use webtor_rs::SnowflakePtMgr;

/// Entry point for WASM
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    // Set up console logging for WASM
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    // Spawn the async main function
    wasm_bindgen_futures::spawn_local(async_main());
}

/// Main async function
#[cfg(target_arch = "wasm32")]
async fn async_main() {
    use tracing::info;

    info!("Starting arti-client with Snowflake transport...");

    match run_tor_client().await {
        Ok(()) => info!("Tor client finished successfully"),
        Err(e) => tracing::error!("Tor client error: {}", e),
    }
}

/// Run the Tor client
#[cfg(target_arch = "wasm32")]
async fn run_tor_client() -> Result<(), Box<dyn std::error::Error>> {
    use arti_client::{TorClient, TorClientConfig};
    use arti_client::config::{BridgeConfigBuilder, CfgPath, pt::TransportConfigBuilder};
    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use tor_rtcompat::wasm::WasmRuntime;
    use tracing::info;

    // =========================================================================
    // Step 1: Create Snowflake PT Manager
    // =========================================================================
    //
    // The SnowflakePtMgr provides Snowflake transport for arti-client.
    // It implements AbstractPtMgr and will intercept "snowflake" transport requests.

    let snowflake_mgr = SnowflakePtMgr::websocket_default();
    info!("Created Snowflake PT manager");

    // =========================================================================
    // Step 2: Configure arti-client with Snowflake bridge
    // =========================================================================
    //
    // Configure a bridge that uses the "snowflake" transport.
    // The actual address (0.0.2.0:1) is a placeholder - our PT manager handles connection.

    let mut config_builder = TorClientConfig::builder();

    // Configure storage paths (required by config validation, but not used on WASM
    // since we use in-memory storage)
    config_builder
        .storage()
        .cache_dir(CfgPath::new("/wasm/cache".to_owned()))
        .state_dir(CfgPath::new("/wasm/state".to_owned()));

    // Configure the Snowflake bridge
    // Format: "snowflake <dummy-addr> <fingerprint>"
    const SNOWFLAKE_BRIDGE_LINE: &str =
        "snowflake 0.0.2.0:1 2B280B23E1107BB62ABFC40DDCC8824814F80A72";

    let bridge: BridgeConfigBuilder = SNOWFLAKE_BRIDGE_LINE.parse()?;
    config_builder.bridges().bridges().push(bridge);

    // Add a transport config entry for "snowflake"
    // We use proxy_addr for "unmanaged" transport - our SnowflakePtMgr will actually handle it
    let mut transport = TransportConfigBuilder::default();
    transport
        .protocols(vec!["snowflake".parse()?])
        // Use a dummy proxy_addr - our in-process PT manager intercepts before this is used
        .proxy_addr("127.0.0.1:1".parse()?);
    config_builder.bridges().transports().push(transport);

    let config = config_builder.build()?;
    info!("Configuration built with Snowflake bridge");

    // =========================================================================
    // Step 3: Create TorClient with WASM runtime
    // =========================================================================

    let runtime = WasmRuntime::default();

    // Create client without bootstrapping so we can inject our PT manager
    let tor_client = TorClient::with_runtime(runtime)
        .config(config)
        .create_unbootstrapped()?;

    info!("TorClient created (unbootstrapped)");

    // =========================================================================
    // Step 4: Inject our Snowflake PT manager
    // =========================================================================
    //
    // This requires the `experimental-api` feature to access chanmgr()

    #[cfg(feature = "experimental-api")]
    {
        tor_client.chanmgr().set_pt_mgr(Arc::new(snowflake_mgr));
        info!("Snowflake PT manager injected into ChanMgr");
    }

    #[cfg(not(feature = "experimental-api"))]
    {
        tracing::warn!(
            "experimental-api feature not enabled - cannot inject custom PT manager. \
             Build with --features experimental-api"
        );
        return Ok(());
    }

    // =========================================================================
    // Step 5: Bootstrap the client
    // =========================================================================

    info!("Bootstrapping Tor client via Snowflake...");
    tor_client.bootstrap().await?;
    info!("Bootstrap complete!");

    // =========================================================================
    // Step 6: Make a connection through Tor
    // =========================================================================

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
    info!("Response received ({} bytes):\n{}", buf.len(), &response[..response.len().min(500)]);

    Ok(())
}

/// Native entry point (for testing the example structure)
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("This example is designed for WASM.");
    println!();
    println!("Build with:");
    println!("  cargo build --example wasm_snowflake --target wasm32-unknown-unknown \\");
    println!("      --no-default-features --features pt-client,experimental-api");
    println!();
    println!("For native Tor access, use the regular readme.rs example.");
}

/// WASM entry point - main is required by cargo but wasm_main is the actual entry
#[cfg(target_arch = "wasm32")]
fn main() {
    // The actual entry point is wasm_main() with #[wasm_bindgen(start)]
}