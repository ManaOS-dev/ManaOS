//! Font types and boot-loaded font assets.

/// Font face used for text rendering.
#[derive(Debug, Clone, Copy)]
pub enum Font {
    /// Inter Latin font.
    Inter,
    /// Noto Sans Japanese font.
    NotoSansJP,
}

/// Font binary assets loaded before boot services exit.
pub struct FontAssets {
    /// Inter font bytes.
    pub inter: &'static [u8],
    /// Noto Sans Japanese font bytes.
    pub noto: &'static [u8],
}
