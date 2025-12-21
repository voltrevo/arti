# webtor-rs Features

## Transport Layer

### Snowflake Bridge
Two connection modes for different use cases:

#### WebSocket Mode (Direct)
- Direct WebSocket connection to Snowflake bridge
- Simpler setup, faster connection
- Good for most use cases

#### WebRTC Mode (Volunteer Proxies)
- Connects via Snowflake broker to volunteer proxies
- More censorship resistant (traffic routed through volunteers)
- Uses JSON-encoded SDP offer/answer for signaling
- Automatic ICE candidate gathering

### Shared Protocol Stack
```
WebSocket / WebRTC DataChannel
    |
Turbo (framing + obfuscation)
    |
KCP (reliability + ordering)
    |
SMUX (stream multiplexing)
    |
TLS (link encryption)
    |
Tor Protocol
```

## Tor Protocol

### 3-Hop Circuits
- **Guard**: First relay, protects your IP
- **Middle**: Intermediate relay, adds anonymity
- **Exit**: Final relay, connects to destination

### Circuit Management
- Automatic circuit creation and rotation
- Configurable refresh intervals
- Graceful circuit transitions
- Circuit reuse for persistent connections
- Isolated circuits for enhanced privacy

## Cryptography

### TLS (1.3 + 1.2)
- WebCrypto API (SubtleCrypto) for browser compatibility
- ECDH key exchange (P-256, P-384, P-521) and X25519
- AES-GCM encryption (TLS 1.3 and 1.2)
- ChaCha20-Poly1305 (pure Rust)
- SHA-256/SHA-384 hashing
- TLS 1.3 preferred with automatic TLS 1.2 fallback when needed

### Tor Encryption
- ntor-v3 handshake (modern key exchange)
- CREATE2 cells (current Tor standard)
- Relay encryption with AES-CTR

## Network Features

### HTTP Client
- GET/POST requests through Tor
- HTTPS support with TLS 1.3 (preferred) and TLS 1.2 fallback
- Proper certificate validation (webpki-roots)
- Automatic content decompression

### Consensus
- Automatic consensus fetching from directory authorities
- Embedded consensus for fast startup
- Consensus diff support for bandwidth efficiency
- Relay selection with bandwidth weights

## Browser Compatibility

### WASM Support
- Compiled to WebAssembly for browser execution
- No native dependencies
- Works in modern browsers (Chrome, Firefox, Safari, Edge)

### API
- Simple JavaScript API
- Promise-based async operations
- Event callbacks for status updates
- Logging with configurable verbosity

## Limitations

### TLS Version Compatibility
TLS 1.3 is preferred with automatic TLS 1.2 fallback. Older protocol versions (TLS 1.1 and below) are not supported. A small number of legacy servers may still have compatibility issues.

### WebRTC Required
Snowflake transport requires WebRTC support in the browser.

## Performance

| Metric          | Typical Value                          |
|-----------------|----------------------------------------|
| WASM Bundle     | ~3.6 MB (~1.2 MB gzipped, wasm-opt)    |
| Initial Load    | 2-5 seconds (WASM compilation)         |
| Circuit Creation| 20-60 seconds (3-hop with handshakes)  |
| Request Latency | typically <3 seconds with circuit reuse|
| Memory Usage    | ~50-100 MB at runtime                  |

## Security

- Memory-safe Rust implementation
- Official Arti crates for protocol handling
- Certificate validation for HTTPS
- No direct IP exposure to destinations
