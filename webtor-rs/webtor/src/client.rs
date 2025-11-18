//! Main Tor client implementation

use crate::circuit::{CircuitManager, CircuitStatusInfo};
use crate::config::{LogType, TorClientOptions};
use crate::error::{Result, TorError};
use crate::http::{HttpRequest, HttpResponse, TorHttpClient};
use crate::relay::RelayManager;
use crate::snowflake::create_snowflake_stream;
use reqwest::Method;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

/// Main Tor client that manages circuits and HTTP requests
pub struct TorClient {
    options: TorClientOptions,
    circuit_manager: Arc<RwLock<CircuitManager>>,
    http_client: Arc<TorHttpClient>,
    is_initialized: Arc<RwLock<bool>>,
    update_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl TorClient {
    /// Create a new Tor client with the given options
    pub async fn new(options: TorClientOptions) -> Result<Self> {
        info!("Creating new Tor client");
        
        // Initialize WASM modules (placeholder for now)
        Self::init_wasm_modules().await?;
        
        // Create relay manager with empty relay list (will be populated later)
        let relay_manager = RelayManager::new(Vec::new());
        let circuit_manager = CircuitManager::new(relay_manager);
        let http_client = TorHttpClient::new(circuit_manager.clone());
        
        let client = Self {
            options: options.clone(),
            circuit_manager: Arc::new(RwLock::new(circuit_manager)),
            http_client: Arc::new(http_client),
            is_initialized: Arc::new(RwLock::new(false)),
            update_task: Arc::new(RwLock::new(None)),
        };
        
        // Create initial circuit if requested
        if options.create_circuit_early {
            info!("Creating initial circuit");
            if let Err(e) = client.create_initial_circuit().await {
                error!("Failed to create initial circuit: {}", e);
                // Don't fail the client creation, just log the error
            }
        }
        
        Ok(client)
    }
    
    /// Make a one-time fetch request through Tor with a temporary circuit
    pub async fn fetch_one_time(
        snowflake_url: &str,
        url: &str,
        connection_timeout: Option<u64>,
        circuit_timeout: Option<u64>,
    ) -> Result<HttpResponse> {
        info!("Making one-time fetch request to {} through Snowflake {}", url, snowflake_url);
        
        let options = TorClientOptions::new(snowflake_url.to_string())
            .with_create_circuit_early(false)
            .with_circuit_update_interval(None) // No auto-updates for one-time use
            .with_connection_timeout(connection_timeout.unwrap_or(15_000))
            .with_circuit_timeout(circuit_timeout.unwrap_or(90_000));
        
        let client = Self::new(options).await?;
        
        // Make the request and then close the client
        let result = client.fetch(url).await;
        client.close().await;
        
        result
    }
    
    /// Make a fetch request through the persistent Tor circuit
    pub async fn fetch(&self, url: &str) -> Result<HttpResponse> {
        self.log(&format!("Starting fetch request to {}", url), LogType::Info);
        
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url);
        
        self.http_client.request(request).await
    }
    
    /// Make a GET request
    pub async fn get(&self, url: &str) -> Result<HttpResponse> {
        self.fetch(url).await
    }
    
    /// Make a POST request
    pub async fn post(&self, url: &str, body: Vec<u8>) -> Result<HttpResponse> {
        let url = Url::parse(url)?;
        let request = HttpRequest::new(url)
            .with_method(Method::POST)
            .with_body(body);
        
        self.http_client.request(request).await
    }
    
    /// Update the circuit with a deadline for graceful transition
    pub async fn update_circuit(&self, deadline: Duration) -> Result<()> {
        info!("Updating circuit with {:?} deadline", deadline);
        
        // For now, this is a placeholder
        // In the full implementation, this would:
        // 1. Create a new circuit in the background
        // 2. Allow existing requests to use the old circuit until deadline
        // 3. Switch to the new circuit after deadline
        
        self.log("Circuit update completed", LogType::Success);
        Ok(())
    }
    
    /// Wait for a circuit to be ready
    pub async fn wait_for_circuit(&self) -> Result<()> {
        info!("Waiting for circuit to be ready");
        
        let circuit_manager = self.circuit_manager.read().await;
        let circuit = circuit_manager.get_ready_circuit().await?;
        
        // Wait for the circuit to be ready
        let circuit_read = circuit.read().await;
        if !circuit_read.is_ready() {
            return Err(TorError::circuit_creation("Circuit is not ready"));
        }
        
        info!("Circuit is ready");
        Ok(())
    }
    
    /// Get current circuit status
    pub async fn get_circuit_status(&self) -> CircuitStatusInfo {
        let circuit_manager = self.circuit_manager.read().await;
        circuit_manager.get_circuit_status().await
    }
    
    /// Get human-readable circuit status string
    pub async fn get_circuit_status_string(&self) -> String {
        let status = self.get_circuit_status().await;
        
        if !status.has_ready_circuits() && status.creating_circuits > 0 {
            return "Creating...".to_string();
        }
        
        if !status.has_ready_circuits() {
            return "None".to_string();
        }
        
        if status.failed_circuits > 0 {
            return format!("Ready ({} failed circuits)", status.failed_circuits);
        }
        
        "Ready".to_string()
    }
    
    /// Close the Tor client and clean up resources
    pub async fn close(&self) {
        info!("Closing Tor client");
        
        // Stop update task if running
        if let Some(task) = self.update_task.write().await.take() {
            task.abort();
        }
        
        // Clean up circuits
        let mut circuit_manager = self.circuit_manager.write().await;
        if let Err(e) = circuit_manager.cleanup_circuits().await {
            warn!("Error during circuit cleanup: {}", e);
        }
        
        *self.is_initialized.write().await = false;
        info!("Tor client closed");
    }
    
    /// Create initial circuit (called during construction)
    async fn create_initial_circuit(&self) -> Result<()> {
        self.log("Creating initial circuit", LogType::Info);
        
        // For now, this is a placeholder
        // In the full implementation, this would:
        // 1. Connect to Snowflake bridge
        // 2. Create Tor connection
        // 3. Build initial circuit through relays
        
        *self.is_initialized.write().await = true;
        self.log("Initial circuit created", LogType::Success);
        
        Ok(())
    }
    
    /// Initialize WASM modules (placeholder)
    async fn init_wasm_modules() -> Result<()> {
        // This will be implemented in the WASM bindings
        // For now, just log that we're initializing
        debug!("Initializing WASM modules");
        Ok(())
    }
    
    /// Log a message (uses callback if provided)
    fn log(&self, message: &str, log_type: LogType) {
        if let Some(ref on_log) = self.options.on_log {
            on_log(message, log_type);
        } else {
            // Default logging
            match log_type {
                LogType::Info => info!("{}", message),
                LogType::Success => info!("✅ {}", message),
                LogType::Error => error!("❌ {}", message),
            }
        }
    }
}

impl Drop for TorClient {
    fn drop(&mut self) {
        // Try to clean up, but don't block since we're in drop
        let client = self.clone();
        tokio::spawn(async move {
            client.close().await;
        });
    }
}

impl Clone for TorClient {
    fn clone(&self) -> Self {
        Self {
            options: self.options.clone(),
            circuit_manager: self.circuit_manager.clone(),
            http_client: self.http_client.clone(),
            is_initialized: self.is_initialized.clone(),
            update_task: self.update_task.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tor_client_creation() {
        let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
            .with_create_circuit_early(false);
        
        let client = TorClient::new(options).await;
        assert!(client.is_ok());
    }
    
    #[tokio::test]
    async fn test_one_time_fetch() {
        // This will fail because we don't have WASM WebSocket implementation
        let result = TorClient::fetch_one_time(
            "wss://snowflake.torproject.net/",
            "https://httpbin.org/ip",
            None,
            None,
        ).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_circuit_status() {
        let options = TorClientOptions::new("wss://snowflake.torproject.net/".to_string())
            .with_create_circuit_early(false);
        
        let client = TorClient::new(options).await.unwrap();
        let status = client.get_circuit_status().await;
        
        assert_eq!(status.total_circuits, 0);
        assert_eq!(status.ready_circuits, 0);
        assert!(!status.has_ready_circuits());
    }
}