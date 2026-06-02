use std::convert::Infallible;

pub(crate) struct FakePRNG<'a> {
    bytes: &'a [u8],
}
impl<'a> FakePRNG<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}
impl<'a> rand_core::TryRng for FakePRNG<'a> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        rand_core::utils::next_word_via_fill(self)
    }
    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        rand_core::utils::next_word_via_fill(self)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
        assert!(dest.len() <= self.bytes.len());

        dest.copy_from_slice(&self.bytes[0..dest.len()]);
        self.bytes = &self.bytes[dest.len()..];

        Ok(())
    }
}
impl rand_core::TryCryptoRng for FakePRNG<'_> {}
