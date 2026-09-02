//! Ciphers used to implement the Tor protocols.
//!
//! Fortunately, Tor has managed not to proliferate ciphers.  It only
//! uses AES, and (so far) only uses AES in counter mode.

/// Re-exports implementations of counter-mode AES.
///
/// These ciphers implement the `cipher::StreamCipher` trait, so use
/// the [`cipher`](https://docs.rs/cipher) crate to access them.
#[cfg_attr(docsrs, doc(cfg(true)))]
#[cfg(not(feature = "with-openssl"))]
pub mod aes {
    // These implement StreamCipher.
    /// AES128 in counter mode as used by Tor.
    pub type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

    /// AES256 in counter mode as used by Tor.  
    pub type Aes256Ctr = ctr::Ctr128BE<aes::Aes256>;
}

/// Compatibility layer between OpenSSL and `cipher::StreamCipher`.
///
/// These ciphers implement the `cipher::StreamCipher` trait, so use
/// the [`cipher`](https://docs.rs/cipher) crate to access them.
#[cfg_attr(docsrs, doc(cfg(true)))]
#[cfg(feature = "with-openssl")]
pub mod aes {
    use cipher::common::array::Array;
    use cipher::common::{InnerUser, KeyInit, KeySizeUser};
    use cipher::inout::InOutBuf;
    use cipher::{InnerIvInit, IvSizeUser, StreamCipher, StreamCipherError};
    use openssl::symm::{Cipher, Crypter, Mode};
    use zeroize::{Zeroize, ZeroizeOnDrop};

    /// AES 128 in counter mode as used by Tor.
    pub struct Aes128Ctr(
        /// Underlying openssl crypto context.
        Crypter,
    );

    /// AES 128 key
    #[derive(Zeroize, ZeroizeOnDrop)]
    pub struct Aes128Key([u8; 16]);

    impl KeySizeUser for Aes128Key {
        type KeySize = typenum::consts::U16;
    }

    impl KeyInit for Aes128Key {
        fn new(key: &Array<u8, Self::KeySize>) -> Self {
            Aes128Key((*key).into())
        }
    }

    impl InnerUser for Aes128Ctr {
        type Inner = Aes128Key;
    }

    impl IvSizeUser for Aes128Ctr {
        type IvSize = typenum::consts::U16;
    }

    impl StreamCipher for Aes128Ctr {
        fn check_remaining(&self, _data_len: usize) -> Result<(), StreamCipherError> {
            // NOTE: this is not a sefe pattern in general, but since the underlying counter
            // is 128 bits, we don't need to worry about overflowing it.
            Ok(())
        }

        fn unchecked_apply_keystream_inout(&mut self, mut buf: InOutBuf<'_, '_, u8>) {
            // TODO(nickm): It would be lovely if we could get rid of this copy somehow.
            let in_buf = zeroize::Zeroizing::new(buf.get_in().to_vec());
            self.0
                .update(&in_buf, buf.get_out())
                .expect("OpenSSL AES encryption failed.");
        }

        fn unchecked_write_keystream(&mut self, buf: &mut [u8]) {
            // TODO(nickm): It would be lovely if we could get rid of this vec somehow.
            let z = vec![0; buf.len()];
            self.0
                .update(&z, buf)
                .expect("OpenSSL AES encryption failed.");
        }
    }

    impl InnerIvInit for Aes128Ctr {
        fn inner_iv_init(inner: Self::Inner, iv: &Array<u8, Self::IvSize>) -> Self {
            let crypter = Crypter::new(Cipher::aes_128_ctr(), Mode::Encrypt, &inner.0, Some(iv))
                .expect("openssl error while initializing Aes128Ctr");
            Aes128Ctr(crypter)
        }
    }

    /// AES 256 in counter mode as used by Tor.
    pub struct Aes256Ctr(Crypter);

    /// AES 256 key
    #[derive(Zeroize, ZeroizeOnDrop)]
    pub struct Aes256Key([u8; 32]);

    impl KeySizeUser for Aes256Key {
        type KeySize = typenum::consts::U32;
    }

    impl KeyInit for Aes256Key {
        fn new(key: &Array<u8, Self::KeySize>) -> Self {
            Aes256Key((*key).into())
        }
    }

    impl InnerUser for Aes256Ctr {
        type Inner = Aes256Key;
    }

    impl IvSizeUser for Aes256Ctr {
        type IvSize = typenum::consts::U16;
    }

    impl StreamCipher for Aes256Ctr {
        fn check_remaining(&self, _data_len: usize) -> Result<(), StreamCipherError> {
            // NOTE: this is not a sefe pattern in general, but since the underlying counter
            // is 128 bits, we don't need to worry about overflowing it.
            Ok(())
        }

        fn unchecked_apply_keystream_inout(&mut self, mut buf: InOutBuf<'_, '_, u8>) {
            // TODO(nickm): It would be lovely if we could get rid of this copy somehow.
            let in_buf = zeroize::Zeroizing::new(buf.get_in().to_vec());
            self.0
                .update(&in_buf, buf.get_out())
                .expect("OpenSSL AES encryption failed.");
        }

        fn unchecked_write_keystream(&mut self, buf: &mut [u8]) {
            // TODO(nickm): It would be lovely if we could get rid of this vec somehow.
            let z = vec![0; buf.len()];
            self.0
                .update(&z, buf)
                .expect("OpenSSL AES encryption failed.");
        }
    }

    impl InnerIvInit for Aes256Ctr {
        fn inner_iv_init(inner: Self::Inner, iv: &Array<u8, Self::IvSize>) -> Self {
            let crypter = Crypter::new(Cipher::aes_256_ctr(), Mode::Encrypt, &inner.0, Some(iv))
                .expect("openssl error while initializing Aes256Ctr");
            Aes256Ctr(crypter)
        }
    }
}
