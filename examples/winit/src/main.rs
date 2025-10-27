use anyrender::{Glyph, NullWindowRenderer, PaintScene, WindowRenderer};
use anyrender_skia::SkiaWindowRenderer;
use kurbo::{Affine, Circle, Point, Rect, Stroke};
use peniko::{Blob, Brush, Color, Fill, FontData, color::AlphaColor};
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

struct App {
    render_state: RenderState,
    width: u32,
    height: u32,
    font_data: FontData,
}

type InitialBackend = SkiaWindowRenderer;
// type InitialBackend = VelloWindowRenderer;
// type InitialBackend = VelloHybridWindowRenderer;
// type InitialBackend = VelloCpuWindowRenderer;
// type InitialBackend = VelloCpuSBWindowRenderer;
// type InitialBackend = NullWindowRenderer;

enum Renderer {
    Skia(Box<SkiaWindowRenderer>),
    Null(NullWindowRenderer),
}

impl From<SkiaWindowRenderer> for Renderer {
    fn from(renderer: SkiaWindowRenderer) -> Self {
        Self::Skia(Box::new(renderer))
    }
}
impl From<NullWindowRenderer> for Renderer {
    fn from(renderer: NullWindowRenderer) -> Self {
        Self::Null(renderer)
    }
}

impl Renderer {
    fn is_active(&self) -> bool {
        match self {
            Renderer::Null(r) => r.is_active(),
            Renderer::Skia(r) => r.is_active(),
        }
    }

    fn set_size(&mut self, w: u32, h: u32) {
        match self {
            Renderer::Null(r) => r.set_size(w, h),
            Renderer::Skia(r) => r.set_size(w, h),
        }
    }
}

enum RenderState {
    Active {
        window: Arc<Window>,
        renderer: Renderer,
    },
    Suspended(Option<Arc<Window>>),
}

impl App {
    fn request_redraw(&mut self) {
        let window = match &self.render_state {
            RenderState::Active { window, renderer } => {
                if renderer.is_active() {
                    Some(window)
                } else {
                    None
                }
            }
            RenderState::Suspended(_) => None,
        };

        if let Some(window) = window {
            window.request_redraw();
        }
    }

    fn draw_scene<T: PaintScene>(scene: &mut T, color: Color, font: FontData) {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        scene.stroke(
            &Stroke::new(2.0),
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(5.0, 5.0, 35.0, 35.0),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Circle::new(Point::new(20.0, 20.0), 10.0),
        );

        let glyphs: Vec<Glyph> = vec![Glyph {
            id: 3,
            x: 100f32,
            y: 100f32,
        }];
        scene.draw_glyphs(
            &font,
            12f32,
            true,
            &[],
            Fill::default(),
            Brush::Solid(AlphaColor::from_rgb8(255, 0, 0)),
            1f32,
            Affine::IDENTITY,
            None,
            glyphs.into_iter(),
        );
    }

    fn set_backend<R: WindowRenderer + Into<Renderer>>(
        &mut self,
        mut renderer: R,
        event_loop: &ActiveEventLoop,
    ) {
        let mut window = match &self.render_state {
            RenderState::Active { window, .. } => Some(window.clone()),
            RenderState::Suspended(cached_window) => cached_window.clone(),
        };
        let window = window.take().unwrap_or_else(|| {
            let attr = Window::default_attributes()
                .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                .with_resizable(true)
                .with_title("anyrender + winit demo")
                .with_visible(true)
                .with_active(true);
            Arc::new(event_loop.create_window(attr).unwrap())
        });

        renderer.resume(window.clone(), self.width, self.height);
        let renderer = renderer.into();
        self.render_state = RenderState::Active { window, renderer };
        self.request_redraw();
    }
}

impl ApplicationHandler for App {
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let RenderState::Active { window, .. } = &self.render_state {
            self.render_state = RenderState::Suspended(Some(window.clone()));
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.set_backend(InitialBackend::new(), event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let RenderState::Active { window, renderer } = &mut self.render_state else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(physical_size) => {
                self.width = physical_size.width;
                self.height = physical_size.height;
                renderer.set_size(self.width, self.height);
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let font_data = self.font_data.clone();

                match renderer {
                    Renderer::Skia(r) => {
                        r.render(|p| App::draw_scene(p, Color::from_rgb8(128, 128, 128), font_data))
                    }
                    Renderer::Null(r) => {
                        r.render(|p| App::draw_scene(p, Color::from_rgb8(0, 0, 0), font_data))
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                let font_data = self.font_data.clone();

                match renderer {
                    Renderer::Skia(r) => {
                        r.render(|p| App::draw_scene(p, Color::from_rgb8(128, 128, 128), font_data))
                    }
                    Renderer::Null(r) => {
                        r.render(|p| App::draw_scene(p, Color::from_rgb8(0, 0, 0), font_data))
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Space),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match renderer {
                Renderer::Skia(_) => {
                    self.set_backend(NullWindowRenderer::new(), event_loop);
                }
                Renderer::Null(_) => {
                    self.set_backend(SkiaWindowRenderer::new(), event_loop);
                }
            },
            _ => {}
        }
    }
}

fn main() {
    let mut app = App {
        render_state: RenderState::Suspended(None),
        width: 800,
        height: 600,
        font_data: FontData::new(Blob::new(Arc::new(include_bytes!("../font.ttf"))), 0),
    };

    let event_loop = EventLoop::new().unwrap();
    event_loop
        .run_app(&mut app)
        .expect("Couldn't run event loop");
}
