#[derive(Debug, Clone)]
pub struct RuntimeInstructionContext<'a> {
    pub bytes: &'a [u8],
    pub address: u64,
    pub cursor: usize,
    pub size_mode: u8,
}

impl<'a> RuntimeInstructionContext<'a> {
    pub const fn new(bytes: &'a [u8], address: u64, cursor: usize, size_mode: u8) -> Self {
        Self {
            bytes,
            address,
            cursor,
            size_mode,
        }
    }
}
