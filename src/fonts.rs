// SPDX-FileCopyrightText: 2020 Robin Krahl <robin.krahl@ireas.org>
// SPDX-License-Identifier: Apache-2.0 or MIT

//! Fonts, font families and a font cache.
//!
//! Before you can use a font in a PDF document, you have to load the [`FontData`][] for it, either
//! from a file ([`FontData::load`][]) or from bytes ([`FontData::new`][]).  See the [`rusttype`][]
//! crate for the supported data formats.  Use the [`from_files`][] function to load a font family
//! from a set of files following the default naming conventions.
//!
//! The [`FontCache`][] caches all loaded fonts.  A [`Font`][] is a reference to a cached font in
//! the [`FontCache`][].  A [`FontFamily`][] is a collection of a regular, a bold, an italic and a
//! bold italic font (raw data or cached).
//!
//! Add fonts to a document’s font cache by calling [`Document::add_font_family`][].  This method
//! returns a reference to the cached data that you then can use with the [`Style`][] struct to
//! change the font family of an element.
//!
//! **Note:**  The [`Font`][] and [`FontFamily<Font>`][`FontFamily`] structs are only valid for the
//! [`FontCache`][] they have been created with.  If you dont use the low-level [`render`][] module
//! directly, only use the [`Document::add_font_family`][] method to add fonts!
//!
//! # Internals
//!
//! There are two types of font data: A [`FontData`][] instance stores information about the glyph
//! metrics that is used to calculate the text size.  It can be loaded at any time using the
//! [`FontData::load`][] and [`FontData::new`][] methods.  Once the PDF document is rendered, a
//! [`printpdf::IndirectFontRef`][] is used to draw text in the PDF document.  Before a font can be
//! used in a PDF document, it has to be embedded using the [`FontCache::load_pdf_fonts`][] method.
//!
//! If you use the high-level interface provided by [`Document`][] to generate a PDF document, these
//! steps are done automatically.  You only have to manually populate the font cache if you use the
//! low-level interface in the [`render`][] module.
//!
//! [`render`]: ../render/
//! [`Document`]: ../struct.Document.html
//! [`Document::add_font_family`]: ../struct.Document.html#method.add_font_family
//! [`Style`]: ../style/struct.Style.html
//! [`from_files`]: fn.from_files.html
//! [`FontCache`]: struct.FontCache.html
//! [`FontCache::load_pdf_fonts`]: struct.FontCache.html#method.load_pdf_fonts
//! [`FontData`]: struct.FontData.html
//! [`FontData::new`]: struct.FontData.html#method.new
//! [`FontData::load`]: struct.FontData.html#method.load
//! [`Font`]: struct.Font.html
//! [`FontFamily`]: struct.FontFamily.html
//! [`rusttype`]: https://docs.rs/rusttype
//! [`rusttype::Font`]: https://docs.rs/rusttype/0.8.3/rusttype/struct.Font.html
//! [`printpdf::IndirectFontRef`]: https://docs.rs/printpdf/0.3.2/printpdf/types/plugins/graphics/two_dimensional/font/struct.IndirectFontRef.html

use std::fmt;
use std::fs;
use std::path;

use crate::error::{Error, ErrorKind};
use crate::render;
use crate::style::Style;
use crate::Mm;

/// Stores font data that can be referenced by a [`Font`][] or [`FontFamily`][].
///
/// If you use the high-level interface provided by [`Document`][], you don’t have to access this
/// type.  See the [module documentation](index.html) for details on the internals.
///
/// [`Document`]: ../struct.Document.html
/// [`Font`]: struct.Font.html
/// [`FontFamily`]: struct.FontFamily.html
#[derive(Debug)]
pub struct FontCache {
    fonts: Vec<FontData>,
    pdf_fonts: Vec<printpdf::IndirectFontRef>,
    // We have to use an option because we first have to construct the FontCache before we can load
    // a font, but the default font is always loaded in new, so this options is always some
    // (outside of new).
    default_font_family: Option<FontFamily<Font>>,
}

impl FontCache {
    /// Creates a new font cache with the given default font family.
    pub fn new(default_font_family: FontFamily<FontData>) -> Result<FontCache, Error> {
        let mut font_cache = FontCache {
            fonts: Vec::new(),
            pdf_fonts: Vec::new(),
            default_font_family: None,
        };
        font_cache.default_font_family = Some(font_cache.add_font_family(default_font_family)?);
        Ok(font_cache)
    }

    /// Adds the given font to the cache and returns a reference to it.
    pub fn add_font(&mut self, font_data: FontData) -> Result<Font, Error> {
        let font = Font::new(self.fonts.len(), &font_data.rt_font)?;
        self.fonts.push(font_data);
        Ok(font)
    }

    /// Adds the given font family to the cache and returns a reference to it.
    pub fn add_font_family(
        &mut self,
        family: FontFamily<FontData>,
    ) -> Result<FontFamily<Font>, Error> {
        Ok(FontFamily {
            regular: self.add_font(family.regular)?,
            bold: self.add_font(family.bold)?,
            italic: self.add_font(family.italic)?,
            bold_italic: self.add_font(family.bold_italic)?,
        })
    }

    /// Embeds all loaded fonts into the document generated by the given renderer and caches a
    /// reference to them.
    pub fn load_pdf_fonts(&mut self, renderer: &render::Renderer) -> Result<(), Error> {
        self.pdf_fonts.clear();
        for font in &self.fonts {
            let pdf_font = match &font.raw_data {
                RawFontData::Embedded(data) => renderer.load_font(&data)?,
            };
            self.pdf_fonts.push(pdf_font);
        }
        Ok(())
    }

    /// Returns the default font family for this font cache.
    pub fn default_font_family(&self) -> FontFamily<Font> {
        self.default_font_family
            .expect("Invariant violated: no default font family for FontCache")
    }

    /// Returns a reference to the emebdded PDF font for the given font, if available.
    ///
    /// This method may only be called with [`Font`][] instances that have been created by this
    /// font cache.  PDF fonts are only avaiable if [`load_pdf_fonts`][] has been called.
    ///
    /// [`Font`]: struct.Font.html
    /// [`load_pdf_fonts`]: #method.load_pdf_fonts
    pub fn get_pdf_font(&self, font: Font) -> Option<&printpdf::IndirectFontRef> {
        self.pdf_fonts.get(font.idx)
    }

    /// Returns a reference to the Rusttype font for the given font, if available.
    ///
    /// This method may only be called with [`Font`][] instances that have been created by this
    /// font cache.
    ///
    /// [`Font`]: struct.Font.html
    pub fn get_rt_font(&self, font: Font) -> &rusttype::Font<'static> {
        &self.fonts[font.idx].rt_font
    }
}

/// The data for a font that is cached by a [`FontCache`][].
///
/// [`FontCache`]: struct.FontCache.html
#[derive(Clone, Debug)]
pub struct FontData {
    rt_font: rusttype::Font<'static>,
    raw_data: RawFontData,
}

impl FontData {
    /// Loads a font from the given data.
    ///
    /// The provided data must by readable by [`rusttype`][].
    ///
    /// [`rusttype`]: https://docs.rs/rusttype
    pub fn new(data: Vec<u8>) -> Result<FontData, rusttype::Error> {
        let rt_font = rusttype::Font::from_bytes(data.clone())?;
        Ok(FontData {
            rt_font,
            raw_data: RawFontData::Embedded(data),
        })
    }

    /// Loads the font at the given path.
    ///
    /// The path must point to a file that can be read by [`rusttype`][].
    ///
    /// [`rusttype`]: https://docs.rs/rusttype
    pub fn load(path: impl AsRef<path::Path>) -> Result<FontData, Error> {
        use std::io::Read as _;

        let path = path.as_ref();
        let mut font_file = fs::File::open(path).map_err(|err| {
            Error::new(format!("Failed to open font file {}", path.display()), err)
        })?;
        let mut buf = Vec::new();
        font_file.read_to_end(&mut buf).map_err(|err| {
            Error::new(format!("Failed to read font file {}", path.display()), err)
        })?;
        let font_data = FontData::new(buf).map_err(|err| {
            Error::new(
                format!("Failed to load rusttype font from file {}", path.display()),
                err,
            )
        })?;
        Ok(font_data)
    }
}

#[derive(Clone, Debug)]
enum RawFontData {
    Embedded(Vec<u8>),
}

/// A collection of fonts with different styles.
///
/// See the [module documentation](index.html) for details on the internals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontFamily<T: Clone + fmt::Debug> {
    /// The regular variant of this font family.
    pub regular: T,
    /// The bold variant of this font family.
    pub bold: T,
    /// The italic variant of this font family.
    pub italic: T,
    /// The bold italic variant of this font family.
    pub bold_italic: T,
}

impl<T: Clone + Copy + fmt::Debug + PartialEq> FontFamily<T> {
    /// Returns the font for the given style.
    pub fn get(&self, style: Style) -> T {
        if style.is_bold() && style.is_italic() {
            self.bold_italic
        } else if style.is_bold() {
            self.bold
        } else if style.is_italic() {
            self.italic
        } else {
            self.regular
        }
    }
}

/// A reference to a font cached by a [`FontCache`][].
///
/// See the [module documentation](index.html) for details on the internals.
///
/// [`FontCache`]: struct.FontCache.html
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    idx: usize,
    scale: f32,
    line_height: Mm,
    glyph_height: Mm,
}

impl Font {
    fn new(idx: usize, rt_font: &rusttype::Font<'static>) -> Result<Font, Error> {
        let scale = rt_font.units_per_em();
        if scale == 0 {
            return Err(Error::new(
                "The font is not scalable",
                ErrorKind::InvalidFont,
            ));
        }
        let scale = f32::from(scale);
        let v_metrics = rt_font.v_metrics_unscaled() * (1.0 / scale);
        let glyph_height = v_metrics.ascent - v_metrics.descent;
        let line_height = glyph_height + v_metrics.line_gap;
        Ok(Font {
            idx,
            scale,
            line_height: printpdf::Pt(f64::from(line_height)).into(),
            glyph_height: printpdf::Pt(f64::from(glyph_height)).into(),
        })
    }

    /// Returns the line height for text with this font and the given font size.
    pub fn get_line_height(&self, font_size: u8) -> Mm {
        self.line_height * f64::from(font_size)
    }

    /// Returns the glyph height for text with this font and the given font size.
    pub fn glyph_height(&self, font_size: u8) -> Mm {
        self.glyph_height * f64::from(font_size)
    }

    /// Returns the width of a character with this font and the given font size.
    ///
    /// The given [`FontCache`][] must be the font cache that loaded this font.
    ///
    /// [`FontCache`]: struct.FontCache.html
    pub fn char_width(&self, font_cache: &FontCache, c: char, font_size: u8) -> Mm {
        let glyph = font_cache
            .get_rt_font(*self)
            .glyph(c)
            .standalone()
            .get_data()
            .expect("No data for standalone glyph");
        let width = glyph.unit_h_metrics.advance_width / self.scale * f32::from(font_size);
        Mm::from(printpdf::Pt(f64::from(width)))
    }

    /// Returns the width of a string with this font and the given font size.
    ///
    /// The given [`FontCache`][] must be the font cache that loaded this font.
    ///
    /// [`FontCache`]: struct.FontCache.html
    pub fn str_width(&self, font_cache: &FontCache, s: &str, font_size: u8) -> Mm {
        s.chars()
            .map(|c| self.char_width(font_cache, c, font_size))
            .sum()
    }
}

/// Loads the font family at the given path with the given name.
///
/// This method assumes that at the given path, these files exist and are valid font files:
/// - `{name}-Regular.ttf`
/// - `{name}-Bold.ttf`
/// - `{name}-Italic.ttf`
/// - `{name}-BoldItalic.ttf`
pub fn from_files(dir: impl AsRef<path::Path>, name: &str) -> Result<FontFamily<FontData>, Error> {
    let dir = dir.as_ref();
    Ok(FontFamily {
        regular: FontData::load(&dir.join(format!("{}-Regular.ttf", name)))?,
        bold: FontData::load(&dir.join(format!("{}-Bold.ttf", name)))?,
        italic: FontData::load(&dir.join(format!("{}-Italic.ttf", name)))?,
        bold_italic: FontData::load(&dir.join(format!("{}-BoldItalic.ttf", name)))?,
    })
}
