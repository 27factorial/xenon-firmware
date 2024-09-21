use super::util::CharHasher;
use crate::widget::bitmap::{Bitmap, BitmapRef};
use alloc::vec::Vec;
use core::mem;
use hashbrown::HashMap;
use miniz_oxide::inflate::{decompress_to_vec_zlib, DecompressError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Font {
    metrics: FontMetrics,
    map: HashMap<char, usize, CharHasher>,
    glyphs: Vec<GlyphData>,
}

impl Font {
    pub fn new(metrics: FontMetrics) -> Self {
        Self {
            metrics,
            map: HashMap::with_hasher(Default::default()),
            glyphs: Vec::new(),
        }
    }

    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, FontError> {
        let bytes = bytes.as_ref();

        postcard::from_bytes(bytes).map_err(FontError::Deserialization)
    }

    pub fn from_compressed_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, FontError> {
        let bytes = bytes.as_ref();
        let decompressed = decompress_to_vec_zlib(bytes).map_err(FontError::Decompression)?;
        Self::from_bytes(decompressed)
    }

    pub fn id(&self, c: char) -> Option<GlyphId> {
        self.map.get(&c).copied().map(GlyphId)
    }

    pub fn set_glyph(&mut self, c: char, glyph: GlyphData) -> Option<GlyphData> {
        match self.map.get(&c).copied() {
            Some(index) => {
                let old_glyph = &mut self.glyphs[index];
                Some(mem::replace(old_glyph, glyph))
            }
            None => {
                let index = self.glyphs.len();
                self.glyphs.push(glyph);
                assert!(self.map.insert(c, index).is_none());
                None
            }
        }
    }

    pub fn font_metrics(&self) -> FontMetrics {
        self.metrics
    }

    pub fn glyph_metrics(&self, GlyphId(id): GlyphId) -> GlyphMetrics {
        let glyph = &self.glyphs[id];

        GlyphMetrics {
            width: glyph.width(),
            height: glyph.height(),
            y_offset: glyph.y_offset,
            h_advance: glyph.h_advance,
            v_advance: glyph.v_advance,
        }
    }

    pub fn glyph_black_bitmap(&self, GlyphId(id): GlyphId) -> BitmapRef<'_> {
        let glyph = &self.glyphs[id];

        glyph.black_bitmap.as_ref()
    }

    pub fn glyph_white_bitmap(&self, GlyphId(id): GlyphId) -> BitmapRef<'_> {
        let glyph = &self.glyphs[id];

        glyph.white_bitmap.as_ref()
    }

    pub fn black_glyph(&self, GlyphId(id): GlyphId) -> (GlyphMetrics, BitmapRef<'_>) {
        let glyph = &self.glyphs[id];

        let metrics = GlyphMetrics {
            width: glyph.width(),
            height: glyph.height(),
            y_offset: glyph.y_offset,
            h_advance: glyph.h_advance,
            v_advance: glyph.v_advance,
        };

        (metrics, glyph.black_bitmap.as_ref())
    }

    pub fn white_glyph(&self, GlyphId(id): GlyphId) -> (GlyphMetrics, BitmapRef<'_>) {
        let glyph = &self.glyphs[id];

        let metrics = GlyphMetrics {
            width: glyph.width(),
            height: glyph.height(),
            y_offset: glyph.y_offset,
            h_advance: glyph.h_advance,
            v_advance: glyph.v_advance,
        };

        (metrics, glyph.white_bitmap.as_ref())
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct GlyphData {
    pub y_offset: i32,
    pub h_advance: i32,
    pub v_advance: i32,
    pub black_bitmap: Bitmap,
    pub white_bitmap: Bitmap,
}

impl GlyphData {
    pub fn new(
        y_offset: i32,
        h_advance: i32,
        v_advance: i32,
        black_bitmap: Bitmap,
        white_bitmap: Bitmap,
    ) -> Self {
        Self {
            y_offset,
            h_advance,
            v_advance,
            black_bitmap,
            white_bitmap,
        }
    }

    pub fn width(&self) -> u32 {
        self.white_bitmap.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.white_bitmap.height() as u32
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct GlyphMetrics {
    pub width: u32,
    pub height: u32,
    pub y_offset: i32,
    pub h_advance: i32,
    pub v_advance: i32,
}

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct FontMetrics {
    pub ascent: i32,
    pub descent: i32,
    pub line_gap: i32,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct GlyphId(usize);

#[derive(Debug, Error)]
pub enum FontError {
    #[error("font decompression error: {0}")]
    Decompression(DecompressError),
    #[error("font deserialization error: {0}")]
    Deserialization(postcard::Error),
}
