use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::cursor::CURSOR_SIZE;
use crate::kernel::driver::display::font::{Font, FontAssets};
use ab_glyph::{point, Font as GlyphFont, FontRef, PxScale, ScaleFont};
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

use spin::Mutex;

static GRAPHICS: Mutex<Option<GraphicsDriver>> = Mutex::new(None);

/// Initialize the global framebuffer graphics driver.
pub fn init_global_graphics(
    framebuffer_info: FrameBufferInfo,
    fonts: FontAssets,
    backbuffer_ptr: *mut u8,
) {
    *GRAPHICS.lock() = Some(GraphicsDriver::new(
        framebuffer_info,
        &fonts,
        backbuffer_ptr,
    ));
}

/// Run a closure with the initialized graphics driver.
///
/// # Panics
///
/// Panics if graphics output has not been initialized.
pub fn with_graphics<R>(operation: impl FnOnce(&GraphicsDriver) -> R) -> R {
    let graphics_lock = GRAPHICS.lock();
    let graphics = graphics_lock
        .as_ref()
        .expect("graphics driver must be initialized before drawing");
    operation(graphics)
}

/// Try to run a closure with mutable access to the graphics driver.
pub fn try_with_graphics_mut<R>(operation: impl FnOnce(&mut GraphicsDriver) -> R) -> Option<R> {
    let mut graphics_lock = GRAPHICS.try_lock()?;
    let graphics = graphics_lock.as_mut()?;
    Some(operation(graphics))
}

/// Framebuffer pixel byte order.
#[derive(Debug, Clone, Copy)]
pub enum ColorFormat {
    /// Red, green, blue byte order.
    Rgb,
    /// Blue, green, red byte order.
    Bgr,
}

/// Information required to write pixels into the active framebuffer.
#[derive(Debug, Clone, Copy)]
pub struct FrameBufferInfo {
    /// Base pointer of the physical framebuffer.
    pub base_ptr: *mut u8,
    /// Visible horizontal resolution in pixels.
    pub horizontal_resolution: usize,
    /// Visible vertical resolution in pixels.
    pub vertical_resolution: usize,
    /// Number of pixels between vertically adjacent rows.
    pub stride: usize,
    /// Pixel byte order used by the framebuffer.
    pub format: ColorFormat,
}

/// Framebuffer-backed graphics driver with a software backbuffer.
pub struct GraphicsDriver {
    /// Framebuffer geometry and pixel format.
    pub info: FrameBufferInfo,
    fonts: ParsedFonts,
    /// Pointer to the software backbuffer.
    pub backbuffer_ptr: *mut u8,
    cursor_backup: [u32; CURSOR_SIZE * CURSOR_SIZE],
    /// Last cursor position used for restoring the background.
    pub last_cursor_pos: (usize, usize),
    /// Stride in bytes.
    stride_bytes: usize,
    /// Whether the pixel format is BGR.
    is_bgr: bool,
}

struct ParsedFonts {
    inter: FontRef<'static>,
    noto: FontRef<'static>,
}

impl ParsedFonts {
    fn new(fonts: &FontAssets) -> Self {
        Self {
            inter: FontRef::try_from_slice(fonts.inter)
                .expect("failed to construct Inter font from boot-loaded font asset"),
            noto: FontRef::try_from_slice(fonts.noto)
                .expect("failed to construct Noto Sans JP font from boot-loaded font asset"),
        }
    }

    fn get(&self, font: Font) -> &FontRef<'static> {
        match font {
            Font::Inter => &self.inter,
            Font::NotoSansJP => &self.noto,
        }
    }
}

// SAFETY: The driver is accessed through a spin mutex, and raw framebuffer
// pointers are only used by methods that perform bounds checks.
unsafe impl Send for GraphicsDriver {}
// SAFETY: Shared access is synchronized by the global mutex.
unsafe impl Sync for GraphicsDriver {}

impl GraphicsDriver {
    /// Create a framebuffer graphics driver.
    pub fn new(info: FrameBufferInfo, fonts: &FontAssets, backbuffer_ptr: *mut u8) -> Self {
        Self {
            info,
            fonts: ParsedFonts::new(fonts),
            backbuffer_ptr,
            cursor_backup: [0; CURSOR_SIZE * CURSOR_SIZE],
            last_cursor_pos: (0, 0),
            stride_bytes: info.stride * 4,
            is_bgr: matches!(info.format, ColorFormat::Bgr),
        }
    }

    /// Copy the backbuffer to the actual VRAM (GOP Framebuffer).
    #[allow(dead_code)]
    pub fn flush(&self) {
        let size = self.info.stride * self.info.vertical_resolution * 4;
        // SAFETY: base_ptr and backbuffer_ptr point to valid buffers of at least
        // size bytes established during graphics initialization.
        unsafe {
            core::ptr::copy_nonoverlapping(self.backbuffer_ptr, self.info.base_ptr, size);
        }
    }

    /// Copy a specific rectangular area from the backbuffer to the VRAM.
    pub fn flush_rect(&self, x: usize, y: usize, width: usize, height: usize) {
        let x_clamped = x.min(self.info.horizontal_resolution);
        let y_clamped = y.min(self.info.vertical_resolution);
        let w_clamped = width.min(self.info.horizontal_resolution - x_clamped);
        let h_clamped = height.min(self.info.vertical_resolution - y_clamped);

        if w_clamped == 0 || h_clamped == 0 {
            return;
        }

        let stride = self.info.stride;
        let bytes_per_pixel = 4;
        let row_size = w_clamped * bytes_per_pixel;

        for py in 0..h_clamped {
            let offset = ((y_clamped + py) * stride + x_clamped) * bytes_per_pixel;
            // SAFETY: The rectangle is clamped to framebuffer bounds and row_size
            // covers only the selected row segment.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.backbuffer_ptr.add(offset),
                    self.info.base_ptr.add(offset),
                    row_size,
                );
            }
        }
    }

    /// Save the background area where the cursor will be drawn (from BACKBUFFER).
    pub fn save_cursor_area(&mut self, x: usize, y: usize) {
        self.last_cursor_pos = (x, y);
        for py in 0..CURSOR_SIZE {
            for px in 0..CURSOR_SIZE {
                self.cursor_backup[py * CURSOR_SIZE + px] = self.get_pixel(x + px, y + py);
            }
        }
    }

    /// Restore the background area from the backup buffer (to BACKBUFFER).
    pub fn restore_cursor_area(&self) {
        let (x, y) = self.last_cursor_pos;
        for py in 0..CURSOR_SIZE {
            for px in 0..CURSOR_SIZE {
                let color = self.cursor_backup[py * CURSOR_SIZE + px];
                self.put_pixel(x + px, y + py, color);
            }
        }
    }

    /// Draw a pixel at the specified coordinates (to BACKBUFFER).
    #[allow(clippy::many_single_char_names)]
    pub fn put_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.info.horizontal_resolution || y >= self.info.vertical_resolution {
            return;
        }

        let pixel_offset = y * self.stride_bytes + x * 4;
        let base = self.backbuffer_ptr;

        let (r, g, b) = (
            ((color >> 16) & 0xFF) as u8,
            ((color >> 8) & 0xFF) as u8,
            (color & 0xFF) as u8,
        );

        // SAFETY: Coordinates were checked against framebuffer bounds, and every
        // pixel is represented by four bytes in the configured framebuffer mode.
        unsafe {
            let ptr = base.add(pixel_offset);
            if self.is_bgr {
                *ptr.add(0) = b;
                *ptr.add(1) = g;
                *ptr.add(2) = r;
            } else {
                *ptr.add(0) = r;
                *ptr.add(1) = g;
                *ptr.add(2) = b;
            }
        }
    }

    /// Fill the screen with a vertical gradient (to BACKBUFFER).
    #[allow(dead_code)]
    pub fn clear_gradient(&self) {
        let v_res = self.info.vertical_resolution;
        let h_res = self.info.horizontal_resolution;
        for y in 0..v_res {
            for x in 0..h_res {
                let r = 0;
                let g = 0;
                let b = u32::from(u8::try_from(34 * (v_res - y) / v_res).unwrap_or(255));
                let color = (r << 16) | (g << 8) | b;
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Draw a filled rectangle with a color.
    pub fn draw_filled_rectangle(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: Color,
    ) {
        let color = color.to_u32();
        let end_x = x.saturating_add(width).min(self.info.horizontal_resolution);
        let end_y = y.saturating_add(height).min(self.info.vertical_resolution);

        for py in y.min(end_y)..end_y {
            for px in x.min(end_x)..end_x {
                self.put_pixel(px, py, color);
            }
        }
    }

    /// Draw a rectangle outline with a color.
    #[allow(dead_code)]
    pub fn draw_rectangle(&self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        if width == 0 || height == 0 {
            return;
        }

        let color = color.to_u32();
        let end_x = x.saturating_add(width).min(self.info.horizontal_resolution);
        let end_y = y.saturating_add(height).min(self.info.vertical_resolution);
        if x >= end_x || y >= end_y {
            return;
        }

        let last_x = end_x - 1;
        let last_y = end_y - 1;

        for px in x..end_x {
            self.put_pixel(px, y, color);
            self.put_pixel(px, last_y, color);
        }
        for py in y..end_y {
            self.put_pixel(x, py, color);
            self.put_pixel(last_x, py, color);
        }
    }

    /// Draw a line into the backbuffer using Bresenham's algorithm.
    pub fn draw_line(&self, x1: i32, y1: i32, x2: i32, y2: i32, color: Color) {
        let color = color.to_u32();
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        let mut x = x1;
        let mut y = y1;

        loop {
            if x >= 0 && y >= 0 {
                self.put_pixel(
                    usize::try_from(x).unwrap_or(0),
                    usize::try_from(y).unwrap_or(0),
                    color,
                );
            }
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a raw RGB image (3 bytes per pixel).
    #[allow(dead_code, clippy::many_single_char_names)]
    pub fn draw_image(&self, x: usize, y: usize, width: usize, height: usize, data: &[u8]) {
        let end_x = x.saturating_add(width).min(self.info.horizontal_resolution);
        let end_y = y.saturating_add(height).min(self.info.vertical_resolution);
        let draw_width = end_x.saturating_sub(x);
        let draw_height = end_y.saturating_sub(y);
        let available_pixels = data.len() / 3;
        let requested_pixels = width.saturating_mul(height);
        let pixels_to_draw = available_pixels.min(requested_pixels);

        for py in 0..draw_height {
            for px in 0..draw_width {
                let source_index = py.saturating_mul(width).saturating_add(px);
                if source_index >= pixels_to_draw {
                    return;
                }

                let offset = source_index * 3;
                let r = u32::from(data[offset]);
                let g = u32::from(data[offset + 1]);
                let b = u32::from(data[offset + 2]);
                let color = (r << 16) | (g << 8) | b;
                self.put_pixel(x + px, y + py, color);
            }
        }
    }

    /// Get pixel color from BACKBUFFER.
    #[allow(clippy::many_single_char_names)]
    pub fn get_pixel(&self, x: usize, y: usize) -> u32 {
        if x >= self.info.horizontal_resolution || y >= self.info.vertical_resolution {
            return 0;
        }
        let offset = (y * self.info.stride + x) * 4;
        // SAFETY: Coordinates were checked against framebuffer bounds before the
        // backbuffer pointer is read.
        unsafe {
            let ptr = self.backbuffer_ptr.add(offset);
            match self.info.format {
                ColorFormat::Rgb => {
                    let r = u32::from(*ptr);
                    let g = u32::from(*ptr.add(1));
                    let b = u32::from(*ptr.add(2));
                    (r << 16) | (g << 8) | b
                }
                ColorFormat::Bgr => {
                    let b = u32::from(*ptr);
                    let g = u32::from(*ptr.add(1));
                    let r = u32::from(*ptr.add(2));
                    (r << 16) | (g << 8) | b
                }
            }
        }
    }

    /// Draw text at the specified coordinates with proper alpha blending (to BACKBUFFER).
    #[allow(
        clippy::many_single_char_names,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn draw_text(
        &self,
        font_kind: Font,
        x: usize,
        y: usize,
        scale: f32,
        color: Color,
        text: &str,
    ) {
        let font = self.fonts.get(font_kind);
        let scale = PxScale::from(scale);
        let scaled_font = font.as_scaled(scale);

        let mut caret = point(x as f32, y as f32 + scaled_font.ascent());

        for c in text.chars() {
            if !c.is_control() {
                let glyph = scaled_font.outline_glyph(
                    font.glyph_id(c)
                        .with_scale_and_position(scale, point(caret.x, caret.y)),
                );

                if let Some(glyph) = glyph {
                    let bounds = glyph.px_bounds();
                    glyph.draw(|gx, gy, v| {
                        if v > 0.01 {
                            // SAFETY: Bounds checked against screen resolution before access.
                            // We round the position by adding 0.5 and casting to integer.
                            let px = (bounds.min.x + gx as f32 + 0.5) as i32;
                            let py = (bounds.min.y + gy as f32 + 0.5) as i32;

                            if px >= 0
                                && py >= 0
                                && (px as usize) < self.info.horizontal_resolution
                                && (py as usize) < self.info.vertical_resolution
                            {
                                let ux = px as usize;
                                let uy = py as usize;

                                let bg_color = self.get_pixel(ux, uy);

                                // Blend the text color with the background.
                                // Formula: result = (text * alpha) + (bg * (1 - alpha))
                                // v is the glyph alpha (0.0 to 1.0).
                                let text_color = color.to_u32();
                                let text_r = (text_color >> 16) & 0xFF;
                                let text_g = (text_color >> 8) & 0xFF;
                                let text_b = text_color & 0xFF;

                                let bg_r = (bg_color >> 16) & 0xFF;
                                let bg_g = (bg_color >> 8) & 0xFF;
                                let bg_b = bg_color & 0xFF;

                                // Alpha blending using fixed-point math to be more precise and performant
                                let alpha = ((v * 256.0) as u32).min(256);
                                let inv_alpha = 256 - alpha;

                                let r = (text_r * alpha + bg_r * inv_alpha) / 256;
                                let g = (text_g * alpha + bg_g * inv_alpha) / 256;
                                let b = (text_b * alpha + bg_b * inv_alpha) / 256;

                                let blended_color = (r << 16) | (g << 8) | b;
                                self.put_pixel(ux, uy, blended_color);
                            }
                        }
                    });
                }
                caret.x += scaled_font.h_advance(font.glyph_id(c));
            }
        }
    }
}

/// Extract framebuffer information from the UEFI graphics output protocol.
pub fn get_info(graphics_output: &mut GraphicsOutput) -> FrameBufferInfo {
    let mode_info = graphics_output.current_mode_info();
    let (width, height) = mode_info.resolution();
    let pixel_format = mode_info.pixel_format();
    let format = match pixel_format {
        PixelFormat::Rgb => ColorFormat::Rgb,
        PixelFormat::Bgr | PixelFormat::Bitmask | PixelFormat::BltOnly => ColorFormat::Bgr,
    };

    let mut framebuffer = graphics_output.frame_buffer();
    let info = FrameBufferInfo {
        base_ptr: framebuffer.as_mut_ptr(),
        horizontal_resolution: width,
        vertical_resolution: height,
        stride: mode_info.stride(),
        format,
    };
    crate::log_info!(
        "framebuffer",
        "GOP mode: {}x{} stride={} pixel_format={} mapped_format={:?}",
        info.horizontal_resolution,
        info.vertical_resolution,
        info.stride,
        pixel_format_name(pixel_format),
        info.format
    );
    crate::log_debug!(
        "framebuffer",
        "Framebuffer base={:p} bytes={}",
        info.base_ptr,
        info.stride
            .saturating_mul(info.vertical_resolution)
            .saturating_mul(4)
    );
    if let Some(bitmask) = mode_info.pixel_bitmask() {
        crate::log_debug!(
            "framebuffer",
            "GOP bitmask: red={:#010x} green={:#010x} blue={:#010x} reserved={:#010x}",
            bitmask.red,
            bitmask.green,
            bitmask.blue,
            bitmask.reserved
        );
    }
    if matches!(pixel_format, PixelFormat::Bitmask | PixelFormat::BltOnly) {
        crate::log_warn!(
            "framebuffer",
            "GOP pixel format {} is treated as BGR",
            pixel_format_name(pixel_format)
        );
    }
    info
}

fn pixel_format_name(pixel_format: PixelFormat) -> &'static str {
    match pixel_format {
        PixelFormat::Rgb => "RGB",
        PixelFormat::Bgr => "BGR",
        PixelFormat::Bitmask => "bitmask",
        PixelFormat::BltOnly => "BLT-only",
    }
}
