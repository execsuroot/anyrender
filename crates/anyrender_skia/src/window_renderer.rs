use anyrender::WindowRenderer;
use debug_timer::debug_timer;
use skia_safe::{Color, Surface};
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;

use crate::{profiler, scene::SkiaSceneCache, SkiaScenePainter};

pub(crate) trait SkiaBackend {
    fn set_size(&mut self, width: u32, height: u32);

    fn prepare(&mut self) -> Option<Surface>;

    fn flush(&mut self, surface: Surface);
}

enum RenderState {
    Active(Box<ActiveRenderState>),
    Suspended,
}

struct ActiveRenderState {
    backend: Box<dyn SkiaBackend>,
    scene_cache: SkiaSceneCache,
}

pub struct SkiaWindowRenderer {
    render_state: RenderState,
}

impl Default for SkiaWindowRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SkiaWindowRenderer {
    pub fn new() -> Self {
        let subscriber = tracing_subscriber::registry().with(profiler::ProfilingLayer);
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");

        Self {
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
        #[cfg(target_os = "macos")]
        let backend = crate::metal::MetalBackend::new(window, width, height);
        #[cfg(not(target_os = "macos"))]
        let backend = crate::opengl::OpenGLBackend::new(window, width, height);

        self.render_state = RenderState::Active(Box::new(ActiveRenderState {
            backend: Box::new(backend),
            scene_cache: SkiaSceneCache::default(),
        }))
    }

    fn suspend(&mut self) {
        self.render_state = RenderState::Suspended;
    }

    fn is_active(&self) -> bool {
        matches!(self.render_state, RenderState::Active(..))
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if let RenderState::Active(state) = &mut self.render_state {
            state.backend.set_size(width, height);
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let RenderState::Active(state) = &mut self.render_state else {
            return;
        };

        debug_timer!(timer, feature = "log_frame_times");

        let mut surface = match state.backend.prepare() {
            Some(it) => it,
            None => return,
        };

        timer.record_time("prepare");

        surface.canvas().restore_to_count(1);
        timer.record_time("restore");
        
        surface.canvas().clear(Color::WHITE);
        timer.record_time("clear");

        draw_fn(&mut SkiaScenePainter {
            inner: surface.canvas(),
            cache: &mut state.scene_cache,
        });
        timer.record_time("cmd");

        state.backend.flush(surface);
        timer.record_time("render");

        state.scene_cache.next_gen();
        timer.record_time("cache next gen");

        timer.print_times("skia: ");
        profiler::print_summary();
    }
}
