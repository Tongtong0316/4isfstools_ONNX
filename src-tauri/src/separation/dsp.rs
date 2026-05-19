pub(crate) const DEFAULT_STFT_WINDOW_SIZE: usize = 4096;
pub(crate) const DEFAULT_STFT_HOP_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Gain {
    pub(crate) linear: f32,
}

impl Gain {
    pub(crate) fn unity() -> Self {
        Self { linear: 1.0 }
    }
}
