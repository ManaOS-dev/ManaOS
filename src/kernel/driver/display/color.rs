//! # `kernel::driver::display::color`
//!
//! Color representation and utilities.

/// A color representation in RGBA format (32-bit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    /// Create a new color from R, G, B components.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    /// Create a new color from R, G, B, A components.
    #[allow(dead_code)]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(
            ((a as u32) << 24)
                | ((r as u32) << 16)
                | ((g as u32) << 8)
                | (b as u32),
        )
    }

    /// Convert color to raw u32.
    pub const fn to_u32(self) -> u32 {
        self.0
    }

    /// Predefined colors
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    #[allow(dead_code)]
    pub const RED: Self = Self::rgb(255, 0, 0);
    #[allow(dead_code)]
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    #[allow(dead_code)]
    pub const BLUE: Self = Self::rgb(0, 0, 255);
}
