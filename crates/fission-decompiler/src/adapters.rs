use crate::types::NirSource;

pub trait NativeDecompilerBackend {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String>;
}

pub struct NativeDecompilerSource<'a, T> {
    inner: &'a mut T,
}

impl<'a, T> NativeDecompilerSource<'a, T> {
    pub fn new(inner: &'a mut T) -> Self {
        Self { inner }
    }
}

impl<T> NirSource for NativeDecompilerSource<'_, T>
where
    T: NativeDecompilerBackend,
{
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.inner.get_pcode_json(address)
    }
}
