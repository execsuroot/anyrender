use std::sync::Arc;

use cocoa::{appkit::NSView, base::id as cocoa_id};
use core_graphics_types::geometry::CGSize;
use foreign_types_shared::ForeignType;
use foreign_types_shared::ForeignTypeRef;
use metal_rs::{
    CAMetalDrawable, CommandQueue, Device, DrawableRef, MTLPixelFormat, MetalDrawableRef,
    MetalLayer,
};
use objc::runtime::YES;
use skia_safe::{
    Canvas, Color4f, ColorType, Paint, Point, Rect, Surface,
    gpu::{self, DirectContext, SurfaceOrigin, backend_render_targets, mtl},
    scalar,
};

use crate::window_renderer::SkiaBackend;

pub struct MetalBackend {
    pub window: Arc<dyn anyrender::WindowHandle>,
    pub metal_layer: MetalLayer,
    pub command_queue: CommandQueue,
    pub skia: DirectContext,
    prepared_drawable: Option<*mut CAMetalDrawable>,
}

impl MetalBackend {
    pub fn new(window: Arc<dyn anyrender::WindowHandle>, width: u32, height: u32) -> Self {
        let device = Device::system_default().expect("no device found");

        let metal_layer = {
            let layer = MetalLayer::new();
            layer.set_device(&device);
            layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            layer.set_presents_with_transaction(false);
            // Disabling this option allows Skia's Blend Mode to work.
            // More about: https://developer.apple.com/documentation/quartzcore/cametallayer/1478168-framebufferonly
            layer.set_framebuffer_only(false);

            unsafe {
                let view = match window.window_handle().unwrap().as_raw() {
                    raw_window_handle::RawWindowHandle::AppKit(appkit) => appkit.ns_view.as_ptr(),
                    _ => panic!("Wrong window handle type"),
                } as cocoa_id;
                view.setWantsLayer(YES);
                view.setLayer(layer.as_ref() as *const _ as _);
            }
            layer.set_drawable_size(CGSize::new(width as f64, height as f64));
            layer
        };

        let command_queue = device.new_command_queue();

        let backend = unsafe {
            mtl::BackendContext::new(
                device.as_ptr() as mtl::Handle,
                command_queue.as_ptr() as mtl::Handle,
            )
        };

        let skia_context = gpu::direct_contexts::make_metal(&backend, None).unwrap();

        Self {
            window,
            metal_layer,
            command_queue,
            skia: skia_context,
            prepared_drawable: None,
        }
    }
}

impl SkiaBackend for MetalBackend {
    fn set_size(&mut self, width: u32, height: u32) {
        self.metal_layer
            .set_drawable_size(CGSize::new(width as f64, height as f64));
    }

    fn prepare(&mut self) -> Option<Surface> {
        let Some(drawable) = self.metal_layer.next_drawable() else {
            return None;
        };

        let (drawable_width, drawable_height) = {
            let size = self.metal_layer.drawable_size();
            (size.width as scalar, size.height as scalar)
        };

        let surface = unsafe {
            let texture_info = mtl::TextureInfo::new(drawable.texture().as_ptr() as mtl::Handle);

            let backend_render_target = backend_render_targets::make_mtl(
                (drawable_width as i32, drawable_height as i32),
                &texture_info,
            );

            gpu::surfaces::wrap_backend_render_target(
                &mut self.skia,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::BGRA8888,
                None,
                None,
            )
            .unwrap()
        };

        self.prepared_drawable = Some(drawable.as_ptr());

        Some(surface)
    }

    fn flush(&mut self, surface: Surface) {
        self.skia.flush_and_submit();
        drop(surface);
        let command_buffer = self.command_queue.new_command_buffer();

        // TODO: save drawable
        let drawable = self.prepared_drawable.take().unwrap();
        command_buffer.present_drawable(&*unsafe { MetalDrawableRef::from_ptr(drawable) });
        command_buffer.commit();
    }
}
