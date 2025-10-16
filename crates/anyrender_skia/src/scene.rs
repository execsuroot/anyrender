use anyrender::PaintScene;
use skia_safe::{
    BlendMode, Color, Color4f, Matrix, Paint, PaintCap, PaintJoin, PaintStyle, RRect, Rect,
    Surface, canvas::SaveLayerRec,
};

pub struct SkiaScenePainter<'s> {
    pub(crate) inner: &'s mut Surface,
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
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke(true);
        paint.set_stroke_width(style.width as f32);
        paint.set_stroke_join(match style.join {
            kurbo::Join::Bevel => PaintJoin::Bevel,
            kurbo::Join::Miter => PaintJoin::Miter,
            kurbo::Join::Round => PaintJoin::Round,
        });
        paint.set_stroke_cap(match style.start_cap {
            kurbo::Cap::Butt => PaintCap::Butt,
            kurbo::Cap::Square => PaintCap::Square,
            kurbo::Cap::Round => PaintCap::Round,
        });

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
        // ToDo: implement draw glyhps
    }

    fn draw_box_shadow(
        &mut self,
        transform: kurbo::Affine,
        rect: kurbo::Rect,
        brush: peniko::Color,
        radius: f64,
        std_dev: f64,
    ) {
        // ToDo: implement draw box shadow
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
