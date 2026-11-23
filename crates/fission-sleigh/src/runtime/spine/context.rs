use std::{cell::Cell, ops::Range};

#[derive(Debug, Clone)]
pub struct RuntimeInstructionContext<'a> {
    bytes: &'a [u8],
    read_end: Option<&'a Cell<usize>>,
    pub address: u64,
    pub cursor: usize,
    pub instruction_width_profile: u8,
}

#[cfg(test)]
mod tests {
    use super::RuntimeInstructionContext;
    use std::cell::Cell;

    #[test]
    fn read_end_includes_unsuccessful_candidate_reads() {
        let end = Cell::new(0);
        let mut context = RuntimeInstructionContext::new(&[0x90, 0xc3], 0x1000, 0, 1);
        context.track_reads(Some(&end));
        assert_eq!(context.read_byte(0), Some(0x90));
        assert_eq!(end.get(), 1);
        assert_eq!(context.read_slice(1..4), None);
        assert_eq!(end.get(), 4);
        assert_eq!(context.read_byte(1), Some(0xc3));
        assert_eq!(end.get(), 4);
    }
}

impl<'a> RuntimeInstructionContext<'a> {
    pub const fn new(
        bytes: &'a [u8],
        address: u64,
        cursor: usize,
        instruction_width_profile: u8,
    ) -> Self {
        Self {
            bytes,
            read_end: None,
            address,
            cursor,
            instruction_width_profile,
        }
    }

    pub(crate) fn track_reads(&mut self, read_end: Option<&'a Cell<usize>>) {
        self.read_end = read_end;
    }

    pub(crate) fn read_byte(&self, offset: usize) -> Option<u8> {
        self.record_read_end(offset.saturating_add(1));
        self.bytes.get(offset).copied()
    }

    pub(crate) fn read_slice(&self, range: Range<usize>) -> Option<&[u8]> {
        self.record_read_end(range.end);
        self.bytes.get(range)
    }

    #[inline]
    fn record_read_end(&self, end: usize) {
        if let Some(read_end) = self.read_end {
            // Failed probes matter too: another window could make an earlier
            // rejected constructor match.
            read_end.set(read_end.get().max(end));
        }
    }
}
