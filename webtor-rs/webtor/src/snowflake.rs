//! Snowflake bridge implementation for Tor connections

use crate::error::{Result, TorError};
use crate::websocket::{WebSocketConnection, WebSocketDuplex};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Snowflake bridge connection manager
pub struct SnowflakeBridge {
    websocket_url: String,
    connection_timeout: Duration,
}

impl SnowflakeBridge {
    pub fn new(websocket_url: String, connection_timeout: Duration) -> Self {
        Self {
            websocket_url,
            connection_timeout,
        }
    }
    
    /// Connect to the Snowflake bridge
    pub async fn connect(&self) -> Result<SnowflakeStream> {
        info!("Connecting to Snowflake bridge at {}", self.websocket_url);
        
        let duplex = WebSocketDuplex::new(
            self.websocket_url.clone(),
            self.connection_timeout,
        );
        
        let connection = duplex.connect().await?;
        
        Ok(SnowflakeStream {
            connection,
            _private: (),
        })
    }
}

/// Snowflake stream for Tor communication
pub struct SnowflakeStream {
    connection: WebSocketConnection,
    _private: (),
}

impl SnowflakeStream {
    /// Send data through the Snowflake stream
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        debug!("Sending {} bytes through Snowflake stream", data.len());
        self.connection.send(data).await
    }
    
    /// Receive data from the Snowflake stream
    pub async fn receive(&mut self) -> Result<Vec<u8>> {
        let data = self.connection.receive().await?;
        debug!("Received {} bytes from Snowflake stream", data.len());
        Ok(data)
    }
    
    /// Close the Snowflake stream
    pub fn close(&mut self) {
        info!("Closing Snowflake stream");
        self.connection.close();
    }
    
    /// Check if the stream is still open
    pub fn is_open(&self) -> bool {
        self.connection.is_open()
    }
}

/// Create a new Snowflake stream (convenience function)
pub async fn create_snowflake_stream(
    websocket_url: &str,
    connection_timeout: Duration,
) -> Result<SnowflakeStream> {
    let bridge = SnowflakeBridge::new(
        websocket_url.to_string(),
        connection_timeout,
    );
    bridge.connect().await
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_snowflake_bridge_creation() {
        let bridge = SnowflakeBridge::new(
            "wss://snowflake.torproject.net/".to_string(),
            Duration::from_secs(15),
        );
        
        // This will fail in native Rust, but should work in WASM
        let result = bridge.connect().await;
        assert!(result.is_err());
    }
}