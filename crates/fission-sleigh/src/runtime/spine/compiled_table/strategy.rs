use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeDecodeStrategy;

impl RuntimeDecodeStrategy {
    pub(super) fn for_table() -> Self {
        Self
    }
}
