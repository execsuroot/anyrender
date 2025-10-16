use std::collections::HashMap;

use anyrender::PaintScene;
use peniko::StyleRef;
use skia_safe::{
    canvas::{GlyphPositions, SaveLayerRec}, font::Edging, font_arguments::{variation_position::Coordinate, VariationPosition}, image_filters::{self, CropRect}, utils::CustomTypefaceBuilder, BlendMode, Color, Color4f, Font, FontArguments, FontHinting, FontMgr, GlyphId, Handle, Matrix, Paint, PaintCap, PaintJoin, PaintStyle, Point, RRect, Rect, Surface, Typeface
};

pub struct SkiaScenePainter<'a> {
    pub(crate) inner: &'a mut Surface,
    pub(crate) font_mgr: &'a mut FontMgr,
    pub(crate) typeface_cache: &'a mut HashMap<u32, Typeface>,
}

impl PaintScene for SkiaScenePainter<'_> {
    fn reset(&mut self) {
        self.inner.canvas().clear(Color::WHITE);
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: kurbo::Affine,
        clip: &impl kurbo::Shape,
    ) {
        let mut paint = Paint::default();
        paint.set_alpha_f(alpha);
        paint.set_anti_alias(true);
        paint.set_blend_mode(peniko_blend_to_skia_blend(blend.into()));

        self.inner
            .canvas()
            .save_layer(&SaveLayerRec::default().paint(&paint));

        self.inner
            .canvas()
            .set_matrix(&kurbo_affine_to_skia_matrix(transform).into());

        self.inner
            .canvas()
            .clip_path(&kurbo_shape_to_skia_path(clip), None, None);
    }

    fn pop_layer(&mut self) {
        self.inner.canvas().restore();
        self.inner.canvas().restore();
    }

    fn stroke<'a>(
        &mut self,
        style: &kurbo::Stroke,
        transform: kurbo::Affine,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_transform: Option<kurbo::Affine>,
        shape: &impl kurbo::Shape,
    ) {
        self.inner.canvas().save();
        self.inner
            .canvas()
            .set_matrix(&kurbo_affine_to_skia_matrix(transform).into());
        // if let Some(affine) = brush_transform {
        //     self.inner
        //         .canvas()
        //         .set_matrix(&kurbo_affine_to_skia_matrix(affine));
        // };

        let mut paint = anyrender_brush_to_skia_paint(brush.into());
        apply_peniko_style_to_skia_paint(StyleRef::Stroke(style), &mut paint);
        paint.set_anti_alias(true);

        draw_kurbo_shape_to_skia_canvas(self.inner.canvas(), shape, &paint);

        self.inner.canvas().restore();
    }

    fn fill<'a>(
        &mut self,
        style: peniko::Fill,
        transform: kurbo::Affine,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_transform: Option<kurbo::Affine>,
        shape: &impl kurbo::Shape,
    ) {
        self.inner.canvas().save();
        self.inner
            .canvas()
            .set_matrix(&kurbo_affine_to_skia_matrix(transform).into());
        // if let Some(affine) = brush_transform {
        //     self.inner
        //         .canvas()
        //         .set_matrix(&kurbo_affine_to_skia_matrix(affine));
        // };

        let mut paint = anyrender_brush_to_skia_paint(brush.into());
        paint.set_anti_alias(true);

        draw_kurbo_shape_to_skia_canvas(self.inner.canvas(), shape, &paint);

        self.inner.canvas().restore();
    }

    fn draw_glyphs<'a, 's: 'a>(
        &'s mut self,
        font: &'a peniko::FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [anyrender::NormalizedCoord],
        style: impl Into<peniko::StyleRef<'a>>,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_alpha: f32,
        transform: kurbo::Affine,
        glyph_transform: Option<kurbo::Affine>,
        glyphs: impl Iterator<Item = anyrender::Glyph>,
    ) {
        self.inner.canvas().save();
        self.inner
            .canvas()
            .set_matrix(&kurbo_affine_to_skia_matrix(transform).into());

        let mut paint = anyrender_brush_to_skia_paint(brush.into());
        apply_peniko_style_to_skia_paint(style.into(), &mut paint);
        paint.set_alpha_f(brush_alpha);
        paint.set_anti_alias(true);

        if !self.typeface_cache.contains_key(&font.index) {
            self.typeface_cache.insert(
                font.index,
                self.font_mgr
                    .new_from_data(&font.data.data(), font.index as usize)
                    .unwrap(),
            );
        }

        let original_typeface = self.typeface_cache.get(&font.index).unwrap();
        let mut normalized_typeface: Option<Typeface> = None;

        if !normalized_coords.is_empty() {
            let axes = original_typeface.variation_design_parameters().unwrap_or(vec![]);
            if !axes.is_empty() {
                let coordinates: Vec<Coordinate> = axes.iter()
                    .zip(normalized_coords.iter())
                    .map(|(axis_param, &raw_value)| {
                        let value = raw_value as f32 / 16384.0; // f2dot14

                        Coordinate {
                            axis: axis_param.tag,
                            value
                        }
                    })
                    .collect();
                let variation_position = VariationPosition { coordinates: &coordinates };

                normalized_typeface = Some(
                    original_typeface.clone_with_arguments(
                        &FontArguments::new().set_variation_design_position(variation_position)
                    ).unwrap()
                );
            }
        }

        let typeface = match &normalized_typeface {
            Some(it) => it,
            None => original_typeface,
        };

        let mut font = Font::from_typeface(typeface, font_size);
        font.set_hinting(if hint {
            FontHinting::Normal
        } else {
            FontHinting::None
        });
        font.set_edging(Edging::SubpixelAntiAlias);

        let mut draw_glyphs: Vec<GlyphId> = vec![];
        let mut draw_positions: Vec<Point> = vec![];

        for glyph in glyphs {
            draw_glyphs.push(GlyphId::from(glyph.id as u16));
            draw_positions.push(Point::new(glyph.x, glyph.y));
        }

        self.inner.canvas().draw_glyphs_at(
            &draw_glyphs[..],
            GlyphPositions::Points(&draw_positions[..]),
            Point::new(0.0, 0.0),
            &font,
            &paint,
        );

        self.inner.canvas().restore();
    }

    fn draw_box_shadow(
        &mut self,
        transform: kurbo::Affine,
        rect: kurbo::Rect,
        brush: peniko::Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.inner.canvas().save();
        self.inner
            .canvas()
            .set_matrix(&kurbo_affine_to_skia_matrix(transform).into());

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(
            Color4f::new(
                brush.components[0],
                brush.components[1],
                brush.components[2],
                brush.components[3],
            ),
            None,
        );
        paint.set_style(PaintStyle::Fill);

        paint.set_image_filter(
            image_filters::blur(
                (std_dev as f32, std_dev as f32),
                None,
                None,
                CropRect::NO_CROP_RECT,
            )
            .unwrap(),
        );

        let rrect = RRect::new_nine_patch(
            Rect::new(
                rect.x0 as f32,
                rect.y0 as f32,
                rect.x1 as f32,
                rect.y1 as f32,
            ),
            radius as f32,
            radius as f32,
            radius as f32,
            radius as f32,
        );

        self.inner.canvas().draw_rrect(&rrect, &paint);

        self.inner.canvas().restore();
    }
}

fn apply_peniko_style_to_skia_paint<'a>(style: peniko::StyleRef<'a>, paint: &mut Paint) {
    match style {
        peniko::StyleRef::Fill(_) => {
            paint.set_style(PaintStyle::Fill);
        }
        peniko::StyleRef::Stroke(stroke) => {
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke(true);
            paint.set_stroke_width(stroke.width as f32);
            paint.set_stroke_join(match stroke.join {
                kurbo::Join::Bevel => PaintJoin::Bevel,
                kurbo::Join::Miter => PaintJoin::Miter,
                kurbo::Join::Round => PaintJoin::Round,
            });
            paint.set_stroke_cap(match stroke.start_cap {
                kurbo::Cap::Butt => PaintCap::Butt,
                kurbo::Cap::Square => PaintCap::Square,
                kurbo::Cap::Round => PaintCap::Round,
            });
        }
    }
}

fn anyrender_brush_to_skia_paint<'a>(brush: anyrender::PaintRef<'a>) -> Paint {
    match brush {
        anyrender::Paint::Solid(alpha_color) => Paint::new(
            Color4f::new(
                alpha_color.components[0],
                alpha_color.components[1],
                alpha_color.components[2],
                alpha_color.components[3],
            ),
            None,
        ),
        anyrender::Paint::Gradient(_) => Paint::default(), // ToDo: implement gradient using paint shader
        anyrender::Paint::Image(_) => Paint::default(), // ToDo: implement image using paint texture
        anyrender::Paint::Custom(_) => unreachable!(),  // ToDo: figure out what to do with this
    }
}

fn kurbo_affine_to_skia_matrix(affine: kurbo::Affine) -> Matrix {
    let m = affine.as_coeffs();
    let scale_x = m[0] as f32;
    let shear_y = m[1] as f32;
    let shear_x = m[2] as f32;
    let scale_y = m[3] as f32;
    let translate_x = m[4] as f32;
    let translate_y = m[5] as f32;

    Matrix::new_all(
        scale_x,
        shear_x,
        translate_x,
        shear_y,
        scale_y,
        translate_y,
        0.0,
        0.0,
        1.0,
    )
}

fn peniko_blend_to_skia_blend(blend_mode: peniko::BlendMode) -> BlendMode {
    if blend_mode.mix == peniko::Mix::Normal || blend_mode.mix == peniko::Mix::Clip {
        match blend_mode.compose {
            peniko::Compose::Clear => BlendMode::Clear,
            peniko::Compose::Copy => BlendMode::Src,
            peniko::Compose::Dest => BlendMode::Dst,
            peniko::Compose::SrcOver => BlendMode::SrcOver,
            peniko::Compose::DestOver => BlendMode::DstOver,
            peniko::Compose::SrcIn => BlendMode::SrcIn,
            peniko::Compose::DestIn => BlendMode::DstIn,
            peniko::Compose::SrcOut => BlendMode::SrcOut,
            peniko::Compose::DestOut => BlendMode::DstOut,
            peniko::Compose::SrcAtop => BlendMode::SrcATop,
            peniko::Compose::DestAtop => BlendMode::DstATop,
            peniko::Compose::Xor => BlendMode::Xor,
            peniko::Compose::Plus => BlendMode::Plus,
            peniko::Compose::PlusLighter => BlendMode::Plus,
        }
    } else {
        match blend_mode.mix {
            peniko::Mix::Normal => unreachable!(), // Handled above
            peniko::Mix::Multiply => BlendMode::Multiply,
            peniko::Mix::Screen => BlendMode::Screen,
            peniko::Mix::Overlay => BlendMode::Overlay,
            peniko::Mix::Darken => BlendMode::Darken,
            peniko::Mix::Lighten => BlendMode::Lighten,
            peniko::Mix::ColorDodge => BlendMode::ColorDodge,
            peniko::Mix::ColorBurn => BlendMode::ColorBurn,
            peniko::Mix::HardLight => BlendMode::HardLight,
            peniko::Mix::SoftLight => BlendMode::SoftLight,
            peniko::Mix::Difference => BlendMode::Difference,
            peniko::Mix::Exclusion => BlendMode::Exclusion,
            peniko::Mix::Hue => BlendMode::Hue,
            peniko::Mix::Saturation => BlendMode::Saturation,
            peniko::Mix::Color => BlendMode::Color,
            peniko::Mix::Luminosity => BlendMode::Luminosity,
            peniko::Mix::Clip => unreachable!(), // Handled above
        }
    }
}

fn draw_kurbo_shape_to_skia_canvas(
    canvas: &skia_safe::Canvas,
    shape: &impl kurbo::Shape,
    paint: &skia_safe::Paint,
) {
    if let Some(rect) = shape.as_rect() {
        canvas.draw_rect(
            Rect::new(
                rect.x0 as f32,
                rect.y0 as f32,
                rect.x1 as f32,
                rect.y1 as f32,
            ),
            paint,
        );
    } else if let Some(rrect) = shape.as_rounded_rect() {
        let rect = Rect::new(
            rrect.rect().x0 as f32,
            rrect.rect().y0 as f32,
            rrect.rect().x1 as f32,
            rrect.rect().y1 as f32,
        );
        canvas.draw_rrect(
            RRect::new_nine_patch(
                rect,
                rrect.radii().bottom_left as f32,
                rrect.radii().top_left as f32,
                rrect.radii().top_right as f32,
                rrect.radii().bottom_right as f32,
            ),
            paint,
        );
    } else if let Some(line) = shape.as_line() {
        canvas.draw_line(
            (line.p0.x as f32, line.p0.y as f32),
            (line.p1.x as f32, line.p1.y as f32),
            paint,
        );
    } else if let Some(circle) = shape.as_circle() {
        canvas.draw_circle(
            (circle.center.x as f32, circle.center.y as f32),
            circle.radius as f32,
            paint,
        );
    } else if let Some(path_els) = shape.as_path_slice() {
        canvas.draw_path(&kurbo_bezpath_els_to_skia_path(path_els), paint);
    } else {
        canvas.draw_path(&kurbo_shape_to_skia_path(shape), paint);
    }
}

fn kurbo_shape_to_skia_path(shape: &impl kurbo::Shape) -> skia_safe::Path {
    let mut sk_path = skia_safe::Path::new();
    for el in shape.path_elements(0.1) {
        add_kurbo_bezpath_el_to_skia_path(&el, &mut sk_path);
    }
    sk_path
}

fn kurbo_bezpath_to_skia_path(path: &kurbo::BezPath) -> skia_safe::Path {
    kurbo_bezpath_els_to_skia_path(path.elements())
}

fn kurbo_bezpath_els_to_skia_path(path: &[kurbo::PathEl]) -> skia_safe::Path {
    let mut sk_path = skia_safe::Path::new();
    for el in path {
        add_kurbo_bezpath_el_to_skia_path(&el, &mut sk_path);
    }
    sk_path
}

fn add_kurbo_bezpath_el_to_skia_path(path_el: &kurbo::PathEl, skia_path: &mut skia_safe::Path) {
    match path_el {
        kurbo::PathEl::MoveTo(p) => _ = skia_path.move_to(skpt(*p)),
        kurbo::PathEl::LineTo(p) => _ = skia_path.line_to(skpt(*p)),
        kurbo::PathEl::QuadTo(p1, p2) => _ = skia_path.quad_to(skpt(*p1), skpt(*p2)),
        kurbo::PathEl::CurveTo(p1, p2, p3) => {
            _ = skia_path.cubic_to(skpt(*p1), skpt(*p2), skpt(*p3))
        }
        kurbo::PathEl::ClosePath => _ = skia_path.close(),
    };
}

fn skpt(p: kurbo::Point) -> skia_safe::Point {
    (p.x as f32, p.y as f32).into()
}
