#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioFormat {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 44_100,
            channels: 2,
        }
    }
}
