use std::{ffi::CString, num::NonZeroU32, sync::Arc};

use anyrender::WindowRenderer;
use debug_timer::debug_timer;
use gl::types::GLint;
use glutin::{
    config::{ConfigTemplateBuilder, GetGlConfig, GlConfig},
    context::ContextAttributesBuilder,
    display::{Display, GetGlDisplay},
    prelude::{GlDisplay, NotCurrentGlContext, PossiblyCurrentGlContext},
    surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface},
};
use skia_safe::Surface;

use crate::scene::SkiaScenePainter;

enum Backend {
    OpenGL {
        gr_context: skia_safe::gpu::DirectContext,
        gl_surface: glutin::surface::Surface<WindowSurface>,
        gl_context: glutin::context::PossiblyCurrentContext,
        fb_info: skia_safe::gpu::gl::FramebufferInfo,
    },
}

impl Backend {
    fn new_gl(window: Arc<dyn anyrender::WindowHandle>, width: u32, height: u32) -> Backend {
        let raw_display_handle = window.display_handle().unwrap().as_raw();
        let raw_window_handle = window.window_handle().unwrap().as_raw();

        let gl_display = unsafe {
            Display::new(
                raw_display_handle,
                #[cfg(target_os = "macos")]
                glutin::display::DisplayApiPreference::Cgl,
                #[cfg(target_os = "windows")]
                glutin::display::DisplayApiPreference::Wgl,
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                glutin::display::DisplayApiPreference::Egl,
            )
            .unwrap()
        };

        let gl_config_template = ConfigTemplateBuilder::new().with_transparency(true).build();
        let gl_config = unsafe {
            gl_display
                .find_configs(gl_config_template)
                .unwrap()
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        };

        let gl_context_attrs = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let gl_surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width).expect("width should be a positive value"),
            NonZeroU32::new(height).expect("height should be a positive value"),
        );

        let gl_not_current_context = unsafe {
            gl_display
                .create_context(&gl_config, &gl_context_attrs)
                .unwrap()
        };

        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &gl_surface_attrs)
                .unwrap()
        };

        let gl_context = gl_not_current_context.make_current(&gl_surface).unwrap();

        gl::load_with(|s| {
            gl_config
                .display()
                .get_proc_address(CString::new(s).unwrap().as_c_str())
        });

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_config
                .display()
                .get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .unwrap();

        let gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None).unwrap();

        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe {
                gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid);
            }

            skia_safe::gpu::gl::FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };

        Backend::OpenGL {
            gr_context,
            gl_surface,
            gl_context,
            fb_info,
        }
    }

    fn create_surface(&mut self, width: u32, height: u32) -> Surface {
        match self {
            Backend::OpenGL {
                gr_context,
                gl_context,
                gl_surface,
                fb_info,
                ..
            } => {
                gl_surface.resize(
                    &gl_context,
                    NonZeroU32::new(width).unwrap(),
                    NonZeroU32::new(height).unwrap(),
                );

                let backend_render_target = skia_safe::gpu::backend_render_targets::make_gl(
                    (width as i32, height as i32),
                    gl_context.config().num_samples() as usize,
                    gl_context.config().stencil_size() as usize,
                    *fb_info,
                );

                skia_safe::gpu::surfaces::wrap_backend_render_target(
                    gr_context,
                    &backend_render_target,
                    skia_safe::gpu::SurfaceOrigin::BottomLeft,
                    skia_safe::ColorType::RGBA8888,
                    None,
                    None,
                )
                .unwrap()
            }
        }
    }

    fn prepare(&self) {
        match self {
            Backend::OpenGL {
                gl_surface,
                gl_context,
                ..
            } => {
                gl_context.make_current(gl_surface).unwrap();
            }
        }
    }

    fn flush_and_submit(&mut self) {
        match self {
            Backend::OpenGL {
                gr_context,
                gl_surface,
                gl_context,
                ..
            } => {
                gr_context.flush_and_submit();
                gl_surface.swap_buffers(&gl_context).unwrap();
            }
        }
    }
}

enum RenderState {
    Active(ActiveRenderState),
    Suspended,
}

struct ActiveRenderState {
    backend: Backend,
    surface: Surface,
}

#[derive(Clone, Default)]
pub struct SkiaRendererOptions {}

pub struct SkiaWindowRenderer {
    options: SkiaRendererOptions,
    render_state: RenderState,
}

impl SkiaWindowRenderer {
    pub fn new() -> Self {
        Self::new_with_options(SkiaRendererOptions::default())
    }

    pub fn new_with_options(options: SkiaRendererOptions) -> Self {
        Self {
            options,
            render_state: RenderState::Suspended,
        }
    }
}

impl SkiaWindowRenderer {}

impl WindowRenderer for SkiaWindowRenderer {
    type ScenePainter<'a>
        = SkiaScenePainter<'a>
    where
        Self: 'a;

    fn resume(&mut self, window: Arc<dyn anyrender::WindowHandle>, width: u32, height: u32) {
        let mut backend = Backend::new_gl(window, width, height);
        let surface = backend.create_surface(width, height);

        self.render_state = RenderState::Active(ActiveRenderState { backend, surface })
    }

    fn suspend(&mut self) {
        self.render_state = RenderState::Suspended;
    }

    fn is_active(&self) -> bool {
        matches!(self.render_state, RenderState::Active(..))
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if let RenderState::Active(state) = &mut self.render_state {
            state.surface = state.backend.create_surface(width, height);
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let RenderState::Active(state) = &mut self.render_state else {
            return;
        };

        debug_timer!(timer, feature = "log_frame_times");

        state.backend.prepare();
        state.surface.canvas().clear(skia_safe::Color::WHITE);

        draw_fn(&mut SkiaScenePainter {
            inner: &mut state.surface,
        });
        timer.record_time("cmd");

        state.backend.flush_and_submit();
        timer.record_time("render");

        timer.print_times("Frame time: ");

        state.surface.canvas().discard();
    }
}
