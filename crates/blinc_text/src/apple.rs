//! Resolve and rasterize native iOS colour fonts through CoreText.

use std::{
    ffi::{CStr, CString, OsStr, c_char, c_void},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTFontURLAttribute: *const c_void;
    fn CTFontCreateWithName(name: *const c_void, size: f64, matrix: *const c_void)
    -> *const c_void;
    fn CTFontGetBoundingRectsForGlyphs(
        font: *const c_void,
        orientation: u32,
        glyphs: *const u16,
        bounds: *mut Rect,
        count: isize,
    ) -> Rect;
    fn CTFontGetAdvancesForGlyphs(
        font: *const c_void,
        orientation: u32,
        glyphs: *const u16,
        advances: *mut Size,
        count: isize,
    ) -> f64;
    fn CTFontDrawGlyphs(
        font: *const c_void,
        glyphs: *const u16,
        positions: *const Point,
        count: usize,
        context: *mut c_void,
    );
    fn CTFontCopyAttribute(font: *const c_void, attribute: *const c_void) -> *const c_void;
    fn CTFontGetGlyphCount(font: *const c_void) -> isize;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        text: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFURLGetFileSystemRepresentation(
        url: *const c_void,
        resolve: u8,
        buffer: *mut u8,
        length: isize,
    ) -> u8;
    fn CFRelease(value: *const c_void);
}

struct Owned(*const c_void);
impl Owned {
    fn new(value: *const c_void) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

pub(crate) fn font_path(name: &str) -> Option<PathBuf> {
    let name = CString::new(name).ok()?;
    // CoreText returns a retained file URL for the resolved native font.
    unsafe {
        let name = Owned::new(CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr(),
            0x08000100,
        ))?;
        let font = Owned::new(CTFontCreateWithName(name.0, 16.0, std::ptr::null()))?;
        let url = Owned::new(CTFontCopyAttribute(font.0, kCTFontURLAttribute))?;
        let mut bytes = [0u8; 4096];
        if CFURLGetFileSystemRepresentation(url.0, 1, bytes.as_mut_ptr(), bytes.len() as isize) == 0
        {
            return None;
        }
        let path = CStr::from_bytes_until_nul(&bytes).ok()?;
        Some(PathBuf::from(OsStr::from_bytes(path.to_bytes())))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    origin: Point,
    size: Size,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *const c_void;
    fn CGColorSpaceRelease(space: *const c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits: usize,
        stride: usize,
        space: *const c_void,
        flags: u32,
    ) -> *mut c_void;
    fn CGContextTranslateCTM(context: *mut c_void, x: f64, y: f64);
    fn CGContextSetShouldSmoothFonts(context: *mut c_void, should_smooth: bool);
    fn CGContextRelease(context: *mut c_void);
}

pub(crate) fn rasterize(
    font: &crate::font::FontFace,
    glyph: u16,
    size: f32,
) -> Option<crate::rasterizer::RasterizedGlyph> {
    use crate::rasterizer::{GlyphFormat, RasterizedGlyph};
    use ttf_parser::GlyphId;
    // Apple's compressed sbix strikes require the platform's decoder.
    let face = font.as_ttf_face()?;
    let name = postscript_name(font)?;
    if !matches!(name.as_str(), "AppleColorEmoji" | ".AppleColorEmojiUI")
        || face.tables().sbix.is_none()
        || face
            .glyph_raster_image(GlyphId(glyph), size.ceil() as u16)
            .is_some()
        || !size.is_finite()
        || size <= 0.0
    {
        return None;
    }
    let name = CString::new(name).ok()?;
    unsafe {
        let name = Owned::new(CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr(),
            0x08000100,
        ))?;
        let native = Owned::new(CTFontCreateWithName(name.0, size as f64, std::ptr::null()))?;
        let bounds = CTFontGetBoundingRectsForGlyphs(native.0, 0, &glyph, std::ptr::null_mut(), 1);
        let left = bounds.origin.x.floor() - 1.0;
        let bottom = bounds.origin.y.floor() - 1.0;
        let top = (bounds.origin.y + bounds.size.height).ceil() + 1.0;
        let width = ((bounds.origin.x + bounds.size.width).ceil() + 1.0 - left) as usize;
        let height = (top - bottom) as usize;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return None;
        }
        let mut advance = Size::default();
        CTFontGetAdvancesForGlyphs(native.0, 0, &glyph, &mut advance, 1);
        let mut bitmap = vec![0u8; width * height * 4];
        let space = CGColorSpaceCreateDeviceRGB();
        if space.is_null() {
            return None;
        }
        // RGBA bytes with premultiplied alpha, independent of host endianness.
        let context = CGBitmapContextCreate(
            bitmap.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            space,
            1 | (4 << 12),
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            return None;
        }
        CGContextTranslateCTM(context, -left, -bottom);
        CTFontDrawGlyphs(native.0, &glyph, &Point::default(), 1, context);
        CGContextRelease(context);
        for pixel in bitmap.as_chunks_mut::<4>().0 {
            if pixel[3] > 0 {
                for i in 0..3 {
                    pixel[i] = ((pixel[i] as u32 * 255 + pixel[3] as u32 / 2) / pixel[3] as u32)
                        .min(255) as u8;
                }
            }
        }
        Some(RasterizedGlyph {
            bitmap,
            width: width as u32,
            height: height as u32,
            bearing_x: left as i16,
            bearing_y: top as i16,
            advance: advance.width.round() as u16,
            format: GlyphFormat::Rgba,
        })
    }
}

/// The face's PostScript name, which Apple's name table may store only as Macintosh ASCII.
fn postscript_name(font: &crate::font::FontFace) -> Option<String> {
    let face = font.as_ttf_face()?;
    face.names().into_iter().find_map(|entry| {
        (entry.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .then(|| {
                entry.to_string().or_else(|| {
                    entry
                        .name
                        .is_ascii()
                        .then(|| String::from_utf8_lossy(entry.name).into_owned())
                })
            })
            .flatten()
    })
}

/// The CoreText font for `font`'s face at `size`, looked up by PostScript name.
///
/// A name that CoreText answers with a different font is refused: glyph ids only mean the same
/// thing in a font with the same glyph count.
fn native_font(font: &crate::font::FontFace, size: f32) -> Option<Owned> {
    let name = CString::new(postscript_name(font)?).ok()?;
    unsafe {
        let name = Owned::new(CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr(),
            0x08000100,
        ))?;
        let native = Owned::new(CTFontCreateWithName(name.0, size as f64, std::ptr::null()))?;
        (CTFontGetGlyphCount(native.0) == font.glyph_count() as isize).then_some(native)
    }
}

/// Draw one glyph of `font` through CoreText into an alpha bitmap.
///
/// For faces whose outlines the software rasterizer cannot read. `None` when
/// CoreText has nothing to draw for the glyph.
pub(crate) fn rasterize_outline(
    font: &crate::font::FontFace,
    glyph: u16,
    size: f32,
    offset_x: f32,
    advance: u16,
) -> Option<crate::rasterizer::RasterizedGlyph> {
    use crate::rasterizer::{GlyphFormat, RasterizedGlyph};
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let native = native_font(font, size)?;
    unsafe {
        let bounds = CTFontGetBoundingRectsForGlyphs(native.0, 0, &glyph, std::ptr::null_mut(), 1);
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return None;
        }
        let shift = offset_x as f64;
        let left = (bounds.origin.x + shift).floor() - 1.0;
        let bottom = bounds.origin.y.floor() - 1.0;
        let top = (bounds.origin.y + bounds.size.height).ceil() + 1.0;
        let right = (bounds.origin.x + bounds.size.width + shift).ceil() + 1.0;
        let (width, height) = ((right - left) as usize, (top - bottom) as usize);
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return None;
        }
        let mut bitmap = vec![0u8; width * height];
        // An alpha-only context (kCGImageAlphaOnly) takes no colour space.
        let context = CGBitmapContextCreate(
            bitmap.as_mut_ptr().cast(),
            width,
            height,
            8,
            width,
            std::ptr::null(),
            7,
        );
        if context.is_null() {
            return None;
        }
        CGContextSetShouldSmoothFonts(context, false);
        CGContextTranslateCTM(context, shift - left, -bottom);
        CTFontDrawGlyphs(native.0, &glyph, &Point::default(), 1, context);
        CGContextRelease(context);
        Some(RasterizedGlyph {
            bitmap,
            width: width as u32,
            height: height as u32,
            bearing_x: left as i16,
            bearing_y: top as i16,
            advance,
            format: GlyphFormat::Alpha,
        })
    }
}
