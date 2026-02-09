//! tor-js: WebAssembly bindings for arti-client
//!
//! This crate provides JavaScript bindings for making HTTP requests through Tor
//! using arti-client (the official Tor Project client library).
//!
//! # Example
//!
//! ```javascript
//! import { init, TorClient, TorClientOptions } from 'tor-js';
//!
//! // Initialize the WASM module
//! init();
//!
//! // Create client options with Snowflake transport
//! // The fingerprint is required for bridge verification
//! const options = new TorClientOptions(
//!   'wss://snowflake.pse.dev/',
//!   '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'  // pse.dev bridge fingerprint
//! );
//!
//! // Create the Tor client (async)
//! const client = await new TorClient(options);
//!
//! // Make a fetch request through Tor
//! const response = await client.fetch('https://check.torproject.org/api/ip');
//! console.log(response.text());
//!
//! // Clean up
//! await client.close();
//! ```

#![cfg(target_arch = "wasm32")]

mod error;
mod fetch;
mod storage;

pub use storage::{JsStorage, JsStorageInterface, JsStateMgr, JsDirStore};

use error::JsTorError;
use fetch::HttpResponse;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use arti_client::config::{BridgeConfigBuilder, CfgPath, pt::TransportConfigBuilder};
use arti_client::{TorClient as ArtiTorClient, TorClientConfig};
use serde::Deserialize;
use tor_rtcompat::wasm::WasmRuntime;
use tracing::{debug, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use wasm_bindgen::prelude::*;
use webtor_rs_lite::arti_transport::{SnowflakeMode, SnowflakePtMgr};

// Global log callback (WASM is single-threaded, so thread_local is fine)
thread_local! {
    static LOG_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the tor-js WASM module
///
/// This must be called before creating any TorClient instances.
/// Sets up panic hooks and logging infrastructure.
#[wasm_bindgen]
pub fn init() -> Result<(), JsValue> {
    // Set up panic handler for better error messages
    console_error_panic_hook::set_once();

    // Set up tracing with custom layer that forwards to JS callback
    let js_layer = JsLogLayer;
    let filter = tracing_subscriber::filter::LevelFilter::DEBUG;

    tracing_subscriber::registry()
        .with(js_layer.with_filter(filter))
        .init();

    info!("tor-js WASM module initialized");
    Ok(())
}

/// Set a callback function to receive log messages
///
/// The callback receives three arguments: (level: string, target: string, message: string)
#[wasm_bindgen(js_name = setLogCallback)]
pub fn set_log_callback(callback: js_sys::Function) {
    LOG_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(callback);
    });
}

/// Custom tracing layer that forwards logs to JavaScript
struct JsLogLayer;

impl<S> tracing_subscriber::Layer<S> for JsLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Extract event data
        let level = event.metadata().level().as_str();
        let target = event.metadata().target();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        // Try to call the JavaScript callback
        LOG_CALLBACK.with(|cb| {
            if let Some(callback) = cb.borrow().as_ref() {
                let _ = callback.call3(
                    &JsValue::NULL,
                    &JsValue::from_str(level),
                    &JsValue::from_str(target),
                    &JsValue::from_str(&visitor.message),
                );
            } else {
                // Fall back to console.log if no callback set
                web_sys::console::log_1(&format!("[{}] {}: {}", level, target, visitor.message).into());
            }
        });
    }
}

/// Visitor to extract message from tracing events
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // Remove surrounding quotes if present
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len()-1].to_string();
            }
        } else if self.message.is_empty() {
            self.message = format!("{} = {:?}", field.name(), value);
        } else {
            self.message.push_str(&format!(", {} = {:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {}", field.name(), value);
        } else {
            self.message.push_str(&format!(", {} = {}", field.name(), value));
        }
    }
}

// ============================================================================
// TorClientOptions
// ============================================================================

/// Options for creating a TorClient
#[wasm_bindgen]
pub struct TorClientOptions {
    mode: SnowflakeMode,
    /// Bridge fingerprint for verification (hex string, 40 chars)
    fingerprint: Option<String>,
    /// Custom storage implementation (optional)
    storage: Option<JsStorageInterface>,
}

#[wasm_bindgen]
impl TorClientOptions {
    /// Create options for WebSocket Snowflake transport
    ///
    /// # Arguments
    /// * `snowflake_url` - WebSocket URL for the Snowflake bridge (e.g., "wss://snowflake.pse.dev/")
    /// * `fingerprint` - Bridge fingerprint (40 char hex string). Required for verification.
    #[wasm_bindgen(constructor)]
    pub fn new(snowflake_url: String, fingerprint: String) -> Self {
        let fp = if fingerprint.is_empty() { None } else { Some(fingerprint) };
        Self {
            mode: SnowflakeMode::WebSocket {
                url: snowflake_url,
                fingerprint: fp.clone(),
            },
            fingerprint: fp,
            storage: None,
        }
    }

    /// Create options for WebRTC Snowflake transport (via broker)
    ///
    /// # Arguments
    /// * `fingerprint` - Bridge fingerprint (40 char hex string). Required for verification.
    #[wasm_bindgen(js_name = snowflakeWebRtc)]
    pub fn snowflake_webrtc(fingerprint: String) -> Self {
        let fp = if fingerprint.is_empty() { None } else { Some(fingerprint) };
        Self {
            mode: SnowflakeMode::WebRtc {
                broker_url: webtor_rs_lite::snowflake_broker::BROKER_URL.to_string(),
                fingerprint: fp.clone(),
            },
            fingerprint: fp,
            storage: None,
        }
    }

    /// Set a custom storage implementation for persistent state.
    ///
    /// When set, the Tor client will persist guard selection and other state
    /// to this storage, allowing faster reconnection across page reloads.
    ///
    /// If not set, in-memory storage is used (state lost on page reload).
    ///
    /// # Arguments
    /// * `storage` - A JavaScript object implementing the TorStorage interface
    #[wasm_bindgen(js_name = withStorage)]
    pub fn with_storage(mut self, storage: JsStorageInterface) -> Self {
        self.storage = Some(storage);
        self
    }
}

// ============================================================================
// TorClient
// ============================================================================

/// Tor client for making HTTP requests through the Tor network
#[wasm_bindgen]
pub struct TorClient {
    inner: Option<Arc<ArtiTorClient<WasmRuntime>>>,
}

#[wasm_bindgen]
impl TorClient {
    /// Create a new TorClient with the given options
    ///
    /// This is an async operation that returns a Promise.
    /// The client will bootstrap and establish a connection to the Tor network.
    #[wasm_bindgen(constructor)]
    pub fn new(options: TorClientOptions) -> js_sys::Promise {
        wasm_bindgen_futures::future_to_promise(async move {
            let client = create_client(options).await?;
            Ok(JsValue::from(client))
        })
    }

    /// Make an HTTP fetch request through Tor
    ///
    /// # Arguments
    /// * `url` - The URL to fetch
    /// * `init` - Optional fetch init options (method, headers, body)
    ///
    /// # Returns
    /// A Promise that resolves to a JsHttpResponse
    #[wasm_bindgen(js_name = fetch, skip_typescript)]
    pub fn fetch(&self, url: String, init: JsValue) -> js_sys::Promise {
        let client = match &self.inner {
            Some(c) => Arc::clone(c),
            None => {
                return wasm_bindgen_futures::future_to_promise(async {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        wasm_bindgen_futures::future_to_promise(async move {
            let response = fetch_impl(&client, &url, init).await?;
            Ok(JsValue::from(response))
        })
    }

    /// Close the TorClient and release resources
    #[wasm_bindgen(js_name = close)]
    pub fn close(&mut self) -> js_sys::Promise {
        self.inner = None;
        wasm_bindgen_futures::future_to_promise(async {
            info!("TorClient closed");
            Ok(JsValue::undefined())
        })
    }
}

/// Create a TorClient with the given options
async fn create_client(options: TorClientOptions) -> Result<TorClient, JsValue> {
    info!("Creating TorClient with arti-client...");

    // Get fingerprint (required for bridge verification)
    let fingerprint = options.fingerprint.clone().ok_or_else(|| {
        JsTorError::config("Bridge fingerprint is required").into_js_value()
    })?;

    // 1. Create Snowflake PT manager from webtor-rs-lite
    let snowflake_mgr = SnowflakePtMgr::new(options.mode);
    info!("Created Snowflake PT manager");

    // 2. Configure arti-client with Snowflake bridge
    let mut config_builder = TorClientConfig::builder();

    // Storage paths (required by config validation, but not used on WASM)
    config_builder
        .storage()
        .cache_dir(CfgPath::new("/wasm/cache".to_owned()))
        .state_dir(CfgPath::new("/wasm/state".to_owned()));

    // Configure the Snowflake bridge with the provided fingerprint
    // Format: "snowflake <dummy-addr> <fingerprint>"
    let bridge_line = format!("snowflake 0.0.2.0:1 {}", fingerprint);
    info!("Using bridge line: {}", bridge_line);

    let bridge: BridgeConfigBuilder = bridge_line
        .parse()
        .map_err(|e| JsTorError::config(format!("Failed to parse bridge line: {}", e)).into_js_value())?;
    config_builder.bridges().bridges().push(bridge);

    // Add transport config for "snowflake"
    let mut transport = TransportConfigBuilder::default();
    transport
        .protocols(vec!["snowflake"
            .parse()
            .map_err(|e| JsTorError::config(format!("Failed to parse protocol: {}", e)).into_js_value())?])
        .proxy_addr(
            "127.0.0.1:1"
                .parse()
                .map_err(|e| JsTorError::config(format!("Failed to parse proxy addr: {}", e)).into_js_value())?,
        );
    config_builder.bridges().transports().push(transport);

    let config = config_builder
        .build()
        .map_err(|e| JsTorError::config(format!("Failed to build config: {}", e)).into_js_value())?;
    info!("Configuration built with Snowflake bridge");

    // 3. Create TorClient with WASM runtime
    let runtime = WasmRuntime::default();

    // Build the client with optional custom storage
    let mut builder = ArtiTorClient::with_runtime(runtime).config(config);

    // Set up custom storage if provided
    if let Some(js_storage_interface) = options.storage {
        info!("Initializing custom JS storage...");
        let js_storage = JsStorage::new(js_storage_interface);

        // Create state manager for client state (guards, circuits)
        let js_statemgr = JsStateMgr::new(js_storage.clone())
            .await
            .map_err(|e| {
                JsTorError::internal(format!("Failed to initialize state storage: {:?}", e)).into_js_value()
            })?;

        // Create dir store for directory cache (consensus, microdescriptors, authcerts)
        let js_dirstore = JsDirStore::new(js_storage, false)
            .await
            .map_err(|e| {
                JsTorError::internal(format!("Failed to initialize directory storage: {:?}", e)).into_js_value()
            })?;

        // Wrap and set on builder
        let boxed_statemgr = tor_persist::BoxedStateMgr::new(js_statemgr);
        let boxed_dirstore = tor_dirmgr::BoxedDirStore::new(js_dirstore);
        builder = builder
            .custom_state_mgr(boxed_statemgr)
            .custom_dir_store(boxed_dirstore);
        info!("Custom storage configured (state + directory cache)");
    } else {
        info!("Using default in-memory storage");
    }

    let tor_client = builder
        .create_unbootstrapped()
        .map_err(|e| JsTorError::internal(format!("Failed to create client: {}", e)).into_js_value())?;

    info!("TorClient created (unbootstrapped)");

    // 4. Inject PT manager (requires experimental-api feature)
    tor_client.chanmgr().set_pt_mgr(Arc::new(snowflake_mgr));
    info!("Snowflake PT manager injected into ChanMgr");

    // 5. Bootstrap the client
    info!("Bootstrapping Tor client via Snowflake...");
    tor_client
        .bootstrap()
        .await
        .map_err(|e| JsTorError::bootstrap(format!("Bootstrap failed: {}", e)).into_js_value())?;
    info!("Bootstrap complete!");

    Ok(TorClient {
        inner: Some(Arc::new(tor_client)),
    })
}

// ============================================================================
// Fetch Implementation
// ============================================================================

/// Fetch init options from JavaScript
#[derive(Debug, Default, Deserialize)]
struct FetchInit {
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    #[serde(skip)]
    body: Option<Vec<u8>>,
    // TODO: support AbortSignal-style cancellation via a `signal` option
}

/// Perform a fetch request
async fn fetch_impl(
    client: &ArtiTorClient<WasmRuntime>,
    url_str: &str,
    init: JsValue,
) -> Result<JsHttpResponse, JsValue> {
    // Parse URL
    let url = url::Url::parse(url_str)
        .map_err(|e| JsTorError::new("INVALID_URL", "validation", e.to_string(), false).into_js_value())?;

    // Parse fetch options
    let mut fetch_init: FetchInit = if init.is_undefined() || init.is_null() {
        FetchInit::default()
    } else {
        serde_wasm_bindgen::from_value(init.clone())
            .map_err(|e| JsTorError::new("INVALID_OPTIONS", "validation", e.to_string(), false).into_js_value())?
    };

    // Extract body separately (handles string, Uint8Array, ArrayBuffer)
    if !init.is_undefined() && !init.is_null() {
        fetch_init.body = extract_body_from_js(&init)?;
    }

    // Parse method
    let method = match fetch_init.method.as_deref() {
        Some("GET") | None => http::Method::GET,
        Some("POST") => http::Method::POST,
        Some("PUT") => http::Method::PUT,
        Some("DELETE") => http::Method::DELETE,
        Some("HEAD") => http::Method::HEAD,
        Some("OPTIONS") => http::Method::OPTIONS,
        Some("PATCH") => http::Method::PATCH,
        Some(other) => {
            return Err(JsTorError::new(
                "INVALID_METHOD",
                "validation",
                format!("Unsupported HTTP method: {}", other),
                false,
            )
            .into_js_value());
        }
    };

    let headers = fetch_init.headers.unwrap_or_default();
    let body = fetch_init.body;

    // Get host and port
    let host = url
        .host_str()
        .ok_or_else(|| JsTorError::new("INVALID_URL", "validation", "No host in URL", false).into_js_value())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| JsTorError::new("INVALID_URL", "validation", "No port in URL", false).into_js_value())?;
    let is_https = url.scheme() == "https";

    info!("Fetching {} via Tor ({}:{})", url, host, port);

    // Connect through Tor
    debug!("Connecting to {}:{}...", host, port);
    let stream = client
        .connect((host, port))
        .await
        .map_err(|e| JsTorError::connection(format!("Failed to connect: {}", e)).into_js_value())?;

    debug!("Connected, making HTTP request...");

    // Perform the HTTP request
    let response = fetch::fetch(stream, &url, method, headers, body, is_https, host)
        .await
        .map_err(|e| e.into_js_value())?;

    Ok(JsHttpResponse::from(response))
}

/// Extract body from JavaScript FetchInit object
fn extract_body_from_js(init: &JsValue) -> Result<Option<Vec<u8>>, JsValue> {
    let body = js_sys::Reflect::get(init, &JsValue::from_str("body"))
        .map_err(|e| JsTorError::new("INVALID_OPTIONS", "validation", format!("Failed to get body: {:?}", e), false).into_js_value())?;

    if body.is_undefined() || body.is_null() {
        return Ok(None);
    }

    // Handle string body
    if let Some(s) = body.as_string() {
        return Ok(Some(s.into_bytes()));
    }

    // Handle Uint8Array
    if let Ok(arr) = body.clone().dyn_into::<js_sys::Uint8Array>() {
        return Ok(Some(arr.to_vec()));
    }

    // Handle ArrayBuffer
    if let Ok(buf) = body.clone().dyn_into::<js_sys::ArrayBuffer>() {
        let arr = js_sys::Uint8Array::new(&buf);
        return Ok(Some(arr.to_vec()));
    }

    Err(JsTorError::new(
        "INVALID_BODY",
        "validation",
        "Body must be a string, Uint8Array, or ArrayBuffer",
        false,
    )
    .into_js_value())
}

// ============================================================================
// JsHttpResponse
// ============================================================================

/// HTTP response exposed to JavaScript
#[wasm_bindgen]
pub struct JsHttpResponse {
    status: u16,
    headers: JsValue,
    body: Vec<u8>,
    url: String,
}

impl From<HttpResponse> for JsHttpResponse {
    fn from(response: HttpResponse) -> Self {
        let headers = serde_wasm_bindgen::to_value(&response.headers)
            .unwrap_or_else(|_| JsValue::from(js_sys::Object::new()));
        Self {
            status: response.status,
            headers,
            body: response.body,
            url: response.url.to_string(),
        }
    }
}

#[wasm_bindgen]
impl JsHttpResponse {
    /// HTTP status code
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Response headers as an object
    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> JsValue {
        self.headers.clone()
    }

    /// Response body as Uint8Array
    #[wasm_bindgen(getter)]
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    /// Final URL (after any redirects)
    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Get response body as text (UTF-8)
    #[wasm_bindgen(js_name = text)]
    pub fn text(&self) -> Result<String, JsValue> {
        String::from_utf8(self.body.clone())
            .map_err(|e| JsTorError::new("INVALID_UTF8", "parse", e.to_string(), false).into_js_value())
    }

    /// Get response body as parsed JSON
    #[wasm_bindgen(js_name = json)]
    pub fn json(&self) -> Result<JsValue, JsValue> {
        let text = self.text()?;
        js_sys::JSON::parse(&text)
            .map_err(|e| JsTorError::new("INVALID_JSON", "parse", format!("{:?}", e), false).into_js_value())
    }
}

// ============================================================================
// TypeScript definitions
// ============================================================================

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = r#"
/**
 * Storage interface for persisting Tor client state.
 *
 * Implement this interface to provide custom storage (IndexedDB, filesystem, etc.).
 * All methods must return Promises.
 *
 * When storage is provided, the Tor client will persist guard selection and other
 * state, allowing faster reconnection across page reloads.
 *
 * @example
 * ```typescript
 * class IndexedDBStorage implements TorStorage {
 *   async get(key: string): Promise<string | null> {
 *     // Load from IndexedDB
 *   }
 *   async set(key: string, value: string): Promise<void> {
 *     // Save to IndexedDB
 *   }
 *   async delete(key: string): Promise<void> {
 *     // Delete from IndexedDB
 *   }
 *   async keys(prefix: string): Promise<string[]> {
 *     // List keys matching prefix
 *   }
 * }
 *
 * const options = new TorClientOptions(url, fingerprint)
 *   .withStorage(new IndexedDBStorage());
 * const client = await new TorClient(options);
 * ```
 */
export interface TorStorage {
    /**
     * Get a value by key.
     * @param key - The storage key
     * @returns The stored value as a string, or null if not found
     */
    get(key: string): Promise<string | null>;

    /**
     * Set a value by key.
     * @param key - The storage key
     * @param value - The value to store (JSON string)
     */
    set(key: string, value: string): Promise<void>;

    /**
     * Delete a value by key.
     * @param key - The storage key
     */
    delete(key: string): Promise<void>;

    /**
     * List all keys with a given prefix.
     * @param prefix - The key prefix to match
     * @returns Array of matching keys
     */
    keys(prefix: string): Promise<string[]>;
}

export interface FetchInit {
    method?: string;
    headers?: Record<string, string>;
    body?: string | Uint8Array | ArrayBuffer;
    // TODO: signal?: AbortSignal;
}

export interface TorClient {
    fetch(url: string, init?: FetchInit): Promise<JsHttpResponse>;
    close(): Promise<void>;
}

export interface TorClientOptions {
    /**
     * Set a custom storage implementation for persistent state.
     * If not provided, in-memory storage is used (state lost on page reload).
     */
    withStorage(storage: TorStorage): TorClientOptions;
}
"#;
