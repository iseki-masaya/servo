extern mod freetype;

use native;
use freetype_impl::font_context::FreeTypeFontContextHandle;
use gfx_font::{
    CSSFontWeight,
    FontHandleMethods,
    FontMetrics,
    FontTable,
    FontTableMethods,
    FontTableTag,
    FractionalPixel,
    SpecifiedFontStyle,
    UsedFontStyle,
    FontWeight100,
    FontWeight200,
    FontWeight300,
    FontWeight400,
    FontWeight500,
    FontWeight600,
    FontWeight700,
    FontWeight800,
    FontWeight900,
};
use geometry;
use geometry::Au;
use text::glyph::GlyphIndex;
use text::util::{float_to_fixed, fixed_to_float};

use self::freetype::freetype::{
    FTErrorMethods,
    FT_F26Dot6,
    FT_Face, 
    FT_FaceRec, 
    FT_GlyphSlot, 
    FT_Library,
    FT_Long,
    FT_ULong,
    FT_SizeRec,
    FT_UInt,
    FT_Size_Metrics,
    FT_STYLE_FLAG_ITALIC,
    FT_STYLE_FLAG_BOLD,
    ft_sfnt_os2
};
use self::freetype::freetype::bindgen::{
    FT_New_Memory_Face,
    FT_Done_Face,
    FT_Get_Char_Index,
    FT_Get_Postscript_Name,
    FT_Load_Glyph,
    FT_Set_Char_Size,
    FT_New_Face,
    FT_Get_Sfnt_Table
};
use self::freetype::tt_os2::TT_OS2;

fn float_to_fixed_ft(f: float) -> i32 {
    float_to_fixed(6, f)
}

fn fixed_to_float_ft(f: i32) -> float {
    fixed_to_float(6, f)
}

pub struct FreeTypeFontTable {
    bogus: ()
}

impl FontTableMethods for FreeTypeFontTable {
    fn with_buffer(&self, _blk: &fn(*u8, uint)) {
        fail!()
    }
}

enum FontSource {
    FontSourceMem(@~[u8]),
    FontSourceFile(~str)
}

pub struct FreeTypeFontHandle {
    // The font binary. This must stay valid for the lifetime of the font,
    // if the font is created using FT_Memory_Face.
    source: FontSource,
    face: FT_Face,
}

#[unsafe_destructor]
impl Drop for FreeTypeFontHandle {
    fn finalize(&self) {
        assert!(self.face.is_not_null());
        if !FT_Done_Face(self.face).succeeded() {
            fail!(~"FT_Done_Face failed");
        }
    }
}

pub impl FreeTypeFontHandle {
    priv fn set_char_size(face: FT_Face, pt_size: float) -> Result<(), ()>{
        let char_width = float_to_fixed_ft(pt_size) as FT_F26Dot6;
        let char_height = float_to_fixed_ft(pt_size) as FT_F26Dot6;
        let h_dpi = 72;
        let v_dpi = 72;

        let result = FT_Set_Char_Size(face, char_width, char_height, h_dpi, v_dpi);
        if result.succeeded() { Ok(()) } else { Err(()) }
    }

    pub fn new_from_file(fctx: &FreeTypeFontContextHandle, file: ~str,
                         style: &SpecifiedFontStyle) -> Result<FreeTypeFontHandle, ()> {
        let ft_ctx: FT_Library = fctx.ctx.ctx;
        if ft_ctx.is_null() { return Err(()); }

        let mut face: FT_Face = ptr::null();
        let face_index = 0 as FT_Long;
        do str::as_c_str(file) |file_str| {
            FT_New_Face(ft_ctx, file_str,
                        face_index, ptr::to_unsafe_ptr(&face));
        }
        if face.is_null() {
            return Err(());
        }
        if FreeTypeFontHandle::set_char_size(face, style.pt_size).is_ok() {
            Ok(FreeTypeFontHandle { source: FontSourceFile(file), face: face })
        } else {
            Err(())
        }
    }

    pub fn new_from_file_unstyled(fctx: &FreeTypeFontContextHandle, file: ~str)
        -> Result<FreeTypeFontHandle, ()> {
        let ft_ctx: FT_Library = fctx.ctx.ctx;
        if ft_ctx.is_null() { return Err(()); }

        let mut face: FT_Face = ptr::null();
        let face_index = 0 as FT_Long;
        do str::as_c_str(file) |file_str| {
            FT_New_Face(ft_ctx, file_str,
                        face_index, ptr::to_unsafe_ptr(&face));
        }
        if face.is_null() {
            return Err(());
        }

        Ok(FreeTypeFontHandle { source: FontSourceFile(file), face: face })
    }

    pub fn new_from_buffer(fctx: &FreeTypeFontContextHandle,
                           buf: @~[u8], style: &SpecifiedFontStyle) -> Result<FreeTypeFontHandle, ()> {
        let ft_ctx: FT_Library = fctx.ctx.ctx;
        if ft_ctx.is_null() { return Err(()); }

        let face_result = do vec::as_imm_buf(*buf) |bytes: *u8, len: uint| {
            create_face_from_buffer(ft_ctx, bytes, len, style.pt_size)
        };

        // TODO: this could be more simply written as result::chain
        // and moving buf into the struct ctor, but cant' move out of
        // captured binding.
        return match face_result {
            Ok(face) => Ok(FreeTypeFontHandle { face: face, source: FontSourceMem(buf) }),
            Err(()) => Err(())
        };

         fn create_face_from_buffer(lib: FT_Library,
                                    cbuf: *u8, cbuflen: uint, pt_size: float) 
             -> Result<FT_Face, ()> {
             
             let mut face: FT_Face = ptr::null();
             let face_index = 0 as FT_Long;
             let result = FT_New_Memory_Face(lib, cbuf, cbuflen as FT_Long,
                                             face_index, ptr::to_unsafe_ptr(&face));
             
             if !result.succeeded() || face.is_null() {
                 return Err(());
             }
             if FreeTypeFontHandle::set_char_size(face, pt_size).is_ok() {
                 Ok(face)
             } else {
                 Err(())
             }
         }
    }
}

impl FontHandleMethods for FreeTypeFontHandle {
    // an identifier usable by FontContextHandle to recreate this FontHandle.
    fn face_identifier(&self) -> ~str {
        /* FT_Get_Postscript_Name seems like a better choice here, but it
           doesn't give usable results for fontconfig when deserializing. */
        unsafe { str::raw::from_c_str((*self.face).family_name) }
    }
    fn family_name(&self) -> ~str {
        unsafe { str::raw::from_c_str((*self.face).family_name) }
    }
    fn face_name(&self) -> ~str {
        unsafe { str::raw::from_c_str(FT_Get_Postscript_Name(self.face)) }
    }
    fn is_italic(&self) -> bool {
        unsafe { (*self.face).style_flags & FT_STYLE_FLAG_ITALIC != 0 }
    }
    fn boldness(&self) -> CSSFontWeight {
        let default_weight = FontWeight400;
        if unsafe { (*self.face).style_flags & FT_STYLE_FLAG_BOLD == 0 } {
            default_weight
        } else {
            let os2 = unsafe { FT_Get_Sfnt_Table(self.face, ft_sfnt_os2) as *TT_OS2 };
            let valid = os2.is_not_null() && unsafe { (*os2).version != 0xffff };
            if valid {
                let weight = unsafe { (*os2).usWeightClass };
                match weight {
                  1 | 100..199 => FontWeight100,
                  2 | 200..299 => FontWeight200,
                  3 | 300..399 => FontWeight300,
                  4 | 400..499 => FontWeight400,
                  5 | 500..599 => FontWeight500,
                  6 | 600..699 => FontWeight600,
                  7 | 700..799 => FontWeight700,
                  8 | 800..899 => FontWeight800,
                  9 | 900..999 => FontWeight900,
                  _ => default_weight
                }
            } else {
                default_weight
            }
        }
    }

    fn clone_with_style(&self,
                        fctx: &native::FontContextHandle,
                        style: &UsedFontStyle) -> Result<FreeTypeFontHandle, ()> {
        match self.source {
            FontSourceMem(buf) => {
                FreeTypeFontHandle::new_from_buffer(fctx, buf, style)
            }
            FontSourceFile(copy file) => {
                FreeTypeFontHandle::new_from_file(fctx, file, style)
            }
        }
    }

    pub fn glyph_index(&self,
                       codepoint: char) -> Option<GlyphIndex> {
        assert!(self.face.is_not_null());
        let idx = FT_Get_Char_Index(self.face, codepoint as FT_ULong);
        return if idx != 0 as FT_UInt {
            Some(idx as GlyphIndex)
        } else {
            debug!("Invalid codepoint: %?", codepoint);
            None
        };
    }

    pub fn glyph_h_advance(&self,
                           glyph: GlyphIndex) -> Option<FractionalPixel> {
        assert!(self.face.is_not_null());
        let res =  FT_Load_Glyph(self.face, glyph as FT_UInt, 0);
        if res.succeeded() {
            unsafe {
                let void_glyph = (*self.face).glyph;
                let slot: FT_GlyphSlot = cast::transmute(void_glyph);
                assert!(slot.is_not_null());
                debug!("metrics: %?", (*slot).metrics);
                let advance = (*slot).metrics.horiAdvance;
                debug!("h_advance for %? is %?", glyph, advance);
                let advance = advance as i32;
                return Some(fixed_to_float_ft(advance) as FractionalPixel);
            }
        } else {
            debug!("Unable to load glyph %?. reason: %?", glyph, res);
            return None;
        }
    }

    pub fn get_metrics(&self) -> FontMetrics {
        /* TODO(Issue #76): complete me */
        let face = self.get_face_rec();

        let underline_size = self.font_units_to_au(face.underline_thickness as float);
        let underline_offset = self.font_units_to_au(face.underline_position as float);
        let em_size = self.font_units_to_au(face.units_per_EM as float);
        let ascent = self.font_units_to_au(face.ascender as float);
        let descent = self.font_units_to_au(face.descender as float);
        let max_advance = self.font_units_to_au(face.max_advance_width as float);

        return FontMetrics {
            underline_size:   underline_size,
            underline_offset: underline_offset,
            leading:          geometry::from_pt(0.0), //FIXME
            x_height:         geometry::from_pt(0.0), //FIXME
            em_size:          em_size,
            ascent:           ascent,
            descent:          descent,
            max_advance:      max_advance
        }
    }

    fn get_table_for_tag(&self, _tag: FontTableTag) -> Option<FontTable> {
        None
    }
}

pub impl FreeTypeFontHandle {
    priv fn get_face_rec(&self) -> &'self FT_FaceRec {
        unsafe {
            &(*self.face)
        }
    }

    priv fn font_units_to_au(&self, value: float) -> Au {

        let face = self.get_face_rec();

        // face.size is a *c_void in the bindings, presumably to avoid
        // recursive structural types
        let size: &FT_SizeRec = unsafe { cast::transmute(&(*face.size)) };
        let metrics: &FT_Size_Metrics = unsafe { &((*size).metrics) };

        let em_size = face.units_per_EM as float;
        let x_scale = (metrics.x_ppem as float) / em_size as float;

        // If this isn't true then we're scaling one of the axes wrong
        assert!(metrics.x_ppem == metrics.y_ppem);

        return geometry::from_frac_px(value * x_scale);
    }
}
