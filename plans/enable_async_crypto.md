# Enable Async AES-GCM for poll_read/poll_write

## Problem

When a server negotiates AES-GCM (common for hosts like google.com), the `poll_read`/`poll_write` methods fail with:
```
"Synchronous decryption only supported for ChaCha20-Poly1305"
```

**Root cause**:
- `poll_read`/`poll_write` are sync methods (return `Poll<...>`, can't use `.await`)
- They call `decrypt_record_sync`/`encrypt_record_sync` which only work for ChaCha20
- AES-GCM uses WebCrypto which is inherently async

**Relevant code**: [stream.rs:554-560](crates/subtle-tls/src/stream.rs#L554-L560)

## Solution: State Machine with Pending Futures

Convert `poll_read`/`poll_write` to poll async crypto futures using a state machine:

1. When decryption needed and no pending future exists:
   - Clone the `CryptoKey` (cheap JS handle clone)
   - Create standalone async decrypt future with owned data
   - Store in `TlsStream.pending_decrypt`

2. Poll the pending future:
   - If `Poll::Ready(Ok(plaintext))`: increment sequence, process result
   - If `Poll::Ready(Err)`: return error
   - If `Poll::Pending`: return `Poll::Pending` (waker will re-invoke)

Same pattern for encryption.

## Implementation

### Step 1: Add cloneable key handle to crypto.rs

```rust
// New: cloneable key handle for AES-GCM
#[derive(Clone)]
pub struct AesGcmKey {
    key: CryptoKey,  // CryptoKey is a JsValue wrapper, cheap to clone
    key_size: usize,
}

impl AesGcm {
    /// Get a cloneable key handle for use in standalone futures
    pub fn key_handle(&self) -> AesGcmKey {
        AesGcmKey {
            key: self.key.clone(),
            key_size: self.key_size,
        }
    }
}

/// Standalone async decrypt function (doesn't borrow Cipher)
pub async fn aes_gcm_decrypt(
    key: &AesGcmKey,
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    // Same as AesGcm::decrypt but with owned key
}
```

### Step 2: Add key extraction to Cipher

```rust
impl Cipher {
    /// Extract key handle for standalone async operations
    pub fn key_handle(&self) -> Option<CipherKeyHandle> {
        match self {
            Cipher::Aes128Gcm(c) | Cipher::Aes256Gcm(c) => Some(CipherKeyHandle::AesGcm(c.key_handle())),
            Cipher::ChaCha20Poly1305(_) => None,  // Uses sync API
        }
    }
}

pub enum CipherKeyHandle {
    AesGcm(AesGcmKey),
}
```

### Step 3: Add pending state to TlsStream

```rust
use std::future::Future;
use std::pin::Pin;

type BoxedDecryptFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + 'static>>;
type BoxedEncryptFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + 'static>>;

pub struct TlsStream<S> {
    // ... existing fields ...

    /// Pending async decryption operation
    pending_decrypt: Option<PendingDecrypt>,
    /// Pending async encryption operation
    pending_encrypt: Option<PendingEncrypt>,
}

struct PendingDecrypt {
    future: BoxedDecryptFuture,
    header: [u8; 5],  // Needed for content type extraction
}

struct PendingEncrypt {
    future: BoxedEncryptFuture,
    original_len: usize,  // Report this many bytes written on completion
}
```

### Step 4: Update poll_read state machine

```rust
fn poll_read(...) -> Poll<io::Result<usize>> {
    // First check if we have a pending decrypt operation
    if let Some(ref mut pending) = self.pending_decrypt {
        match Pin::new(&mut pending.future).poll(cx) {
            Poll::Ready(Ok(plaintext)) => {
                // Increment sequence number
                if let Some(ref mut cipher) = self.record_layer.read_cipher {
                    cipher.increment_sequence();
                }
                let header = pending.header;
                self.pending_decrypt = None;

                // Process decrypted plaintext (extract content type, etc.)
                return self.process_decrypted_record(header, plaintext, buf);
            }
            Poll::Ready(Err(e)) => {
                self.pending_decrypt = None;
                return Poll::Ready(Err(io::Error::new(...)));
            }
            Poll::Pending => return Poll::Pending,
        }
    }

    // ... existing buffer drain logic ...

    // When we have a full encrypted record:
    if self.record_layer.read_cipher.as_ref().map(|c| c.aead.supports_sync()).unwrap_or(true) {
        // ChaCha20 or no cipher: use sync path (existing code)
        match self.record_layer.decrypt_record_sync(&header, body) { ... }
    } else {
        // AES-GCM: start async decrypt
        let cipher = self.record_layer.read_cipher.as_ref().unwrap();
        let nonce = cipher.compute_nonce();
        let key_handle = cipher.aead.key_handle().unwrap();
        let header_owned: [u8; 5] = header.try_into().unwrap();
        let body_owned = body.to_vec();

        let future = Box::pin(async move {
            aes_gcm_decrypt(&key_handle, &nonce, &header_owned, &body_owned).await
        });

        self.pending_decrypt = Some(PendingDecrypt {
            future,
            header: header_owned
        });

        // Immediately try to poll (might complete synchronously in some cases)
        cx.waker().wake_by_ref();
        return Poll::Pending;
    }
}
```

### Step 5: Update poll_write similarly

Same pattern for encryption - if ChaCha20 use sync, if AES-GCM create async future and poll.

## Files to Modify

| File | Changes |
|------|---------|
| `crates/subtle-tls/src/crypto.rs` | Add `AesGcmKey`, `key_handle()`, `CipherKeyHandle`, standalone `aes_gcm_decrypt/encrypt` |
| `crates/subtle-tls/src/stream.rs` | Add `pending_decrypt`/`pending_encrypt` fields, state machine in `poll_read`/`poll_write` |
| `crates/subtle-tls/src/record.rs` | Add `increment_sequence()` method to `RecordCipher` (make accessible) |

## Verification

1. Build: `scripts/tor-js/build.sh`
2. Test with host that negotiates AES-GCM (e.g., google.com):
   ```javascript
   const response = await client.fetch("https://www.google.com/");
   ```
3. Test with host that negotiates ChaCha20 (if any) to ensure no regression
4. Test handshake (uses async path already) still works
