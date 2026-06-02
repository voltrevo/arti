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
#[derive(Copy, Clone, Debug, derive_builder::Builder, amplify::Getters)]
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

impl Default for CommonListenOptions {
    fn default() -> Self {
        // Tested by the `builder_defaults()` test below.
        Self::builder()
            .build()
            .expect("Default builder values panicked")
    }
}

/// Options to use when initializing a TCP listening socket.
///
/// See [`CommonListenOptions`] for more information.
#[derive(Copy, Clone, Debug, derive_builder::Builder, amplify::Getters)]
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

impl Default for TcpListenOptions {
    fn default() -> Self {
        // Tested by the `builder_defaults()` test below.
        Self::builder()
            .build()
            .expect("Default builder values panicked")
    }
}

/// Socket options to set when initializing a unix stream listening socket.
// TODO: We should support at least the options in `CommonListenOptions`.
#[derive(Copy, Clone, Debug, derive_builder::Builder, amplify::Getters)]
#[non_exhaustive]
pub struct UnixListenOptions {}

impl UnixListenOptions {
    /// Returns a builder for this [`UnixListenOptions`].
    pub fn builder() -> UnixListenOptionsBuilder {
        Default::default()
    }
}

impl Default for UnixListenOptions {
    fn default() -> Self {
        // Tested by the `builder_defaults()` test below.
        Self::builder()
            .build()
            .expect("Default builder values panicked")
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
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->

    use super::*;

    #[test]
    fn builder_defaults() {
        // Ensure that the `Default::default()` impl doesn't panic.
        CommonListenOptions::default();
        TcpListenOptions::default();
        UnixListenOptions::default();
    }
}
