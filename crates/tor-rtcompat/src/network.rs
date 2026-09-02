//! Definitions for types used with [`NetStreamProvider`](crate::NetStreamProvider).

/// Options to use when initializing a listening socket.
///
/// This may include both options that affect the listening,
/// and options that will apply to any individual accepted connection streams.
///
/// It can include options set with `setsockopt`,
/// as well as options that influence higher layers (eg, the runtime).
///
/// For established streams that are accepted from a listener,
/// you can use [`StreamOps`](crate::StreamOps) to perform additional operations
/// or to configure additional options.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct CommonListenOptions {
    /// Value set for `SO_SNDBUF` on the listening socket.
    #[builder(default)]
    pub(crate) send_buffer_size: Option<usize>,

    /// Value set for `SO_RCVBUF` on the listening socket.
    #[builder(default)]
    pub(crate) recv_buffer_size: Option<usize>,
}

impl CommonListenOptions {
    /// Returns a builder for this [`CommonListenOptions`].
    pub fn builder() -> CommonListenOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for CommonListenOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {
            send_buffer_size: None,
            recv_buffer_size: None,
        }
    }
}

/// Options to use when initializing a TCP listening socket.
///
/// See [`CommonListenOptions`] for more information.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct TcpListenOptions {
    /// Options that are common for all socket types.
    #[builder(sub_builder)]
    pub(crate) common: CommonListenOptions,
}

impl TcpListenOptions {
    /// Returns a builder for this [`TcpListenOptions`].
    pub fn builder() -> TcpListenOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for TcpListenOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {
            common: CommonListenOptions::default(),
        }
    }
}

/// Options to use when initializing a unix stream listening socket.
// TODO: We should support at least the options in `CommonListenOptions`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct UnixListenOptions {}

impl UnixListenOptions {
    /// Returns a builder for this [`UnixListenOptions`].
    pub fn builder() -> UnixListenOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for UnixListenOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {}
    }
}

/// Options to use when connecting a socket.
///
/// This may include both options that affect the connection attempt,
/// and options that will apply to the resulting connection stream.
///
/// It can include options set with `setsockopt`,
/// as well as options that influence higher layers (eg, the runtime).
///
/// For established streams,
/// you can use [`StreamOps`](crate::StreamOps) to perform additional operations
/// or to configure additional options.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct CommonConnectOptions {
    /// Value set for `SO_SNDBUF` on the socket.
    #[builder(default)]
    pub(crate) send_buffer_size: Option<usize>,

    /// Value set for `SO_RCVBUF` on the socket.
    #[builder(default)]
    pub(crate) recv_buffer_size: Option<usize>,
}

impl CommonConnectOptions {
    /// Returns a builder for this [`CommonConnectOptions`].
    pub fn builder() -> CommonConnectOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for CommonConnectOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {
            send_buffer_size: None,
            recv_buffer_size: None,
        }
    }
}

/// Options to use when connecting a TCP socket.
///
/// See [`CommonConnectOptions`] for more information.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct TcpConnectOptions {
    /// Options that are common for all socket types.
    #[builder(sub_builder)]
    pub(crate) common: CommonConnectOptions,
}

impl TcpConnectOptions {
    /// Returns a builder for this [`TcpConnectOptions`].
    pub fn builder() -> TcpConnectOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for TcpConnectOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {
            common: CommonConnectOptions::default(),
        }
    }
}

/// Options to use when connecting a unix stream socket.
// TODO: We should support at least the options in `CommonConnectOptions`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct UnixConnectOptions {}

impl UnixConnectOptions {
    /// Returns a builder for this [`UnixConnectOptions`].
    pub fn builder() -> UnixConnectOptionsBuilder {
        Default::default()
    }
}

// We want to make sure that the defaults are set to the defaults that the builder uses.
#[allow(clippy::derivable_impls)]
impl Default for UnixConnectOptions {
    fn default() -> Self {
        // This needs to match the result of `Self::builder().build().unwrap()`,
        // which is tested by the `builder_defaults()` test below.
        Self {}
    }
}

#[cfg(test)]
mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_time_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    #![allow(clippy::string_slice)] // See arti#2571
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->

    use super::*;

    #[test]
    fn builder_defaults() {
        // Ensure that the builder default matches the type's default.
        macro_rules! check {
            ($type:tt) => {
                assert_eq!($type::builder().build().unwrap(), $type::default());
            };
        }

        check!(CommonListenOptions);
        check!(TcpListenOptions);
        check!(UnixListenOptions);

        check!(CommonConnectOptions);
        check!(TcpConnectOptions);
        check!(UnixConnectOptions);
    }
}
