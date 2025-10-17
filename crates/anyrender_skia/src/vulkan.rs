use std::sync::Arc;

use skia_safe::{
    Canvas, ColorType, Surface,
    gpu::{
        DirectContext, SurfaceOrigin, direct_contexts,
        ganesh::vk::backend_render_targets,
        surfaces,
        vk::{self, Format},
    },
};
use vulkano::{
    Handle, Validated, VulkanError, VulkanObject,
    device::{Queue, QueueCreateInfo},
    image::view::ImageView,
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass},
    swapchain::{
        Swapchain, SwapchainAcquireFuture, SwapchainCreateInfo, SwapchainPresentInfo,
        acquire_next_image,
    },
    sync::GpuFuture,
};

use crate::window_renderer::SkiaBackend;

pub(crate) struct VulkanBackend {
    gr_context: DirectContext,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain>,
    framebuffers: Vec<Arc<Framebuffer>>,
    render_pass: Arc<RenderPass>,
    prepared: Option<(u32, SwapchainAcquireFuture)>,
    last_render: Option<Box<dyn GpuFuture>>,
    swapchain_is_valid: bool,
    window_size: [u32; 2],
}

impl VulkanBackend {
    pub(crate) fn new(
        window: Arc<dyn anyrender::WindowHandle>,
        width: u32,
        height: u32,
    ) -> VulkanBackend {
        let library = vulkano::VulkanLibrary::new().expect("vulkan library is available on system");

        let required_extensions =
            vulkano::swapchain::Surface::required_extensions(&window).unwrap();

        let instance = vulkano::instance::Instance::new(
            library.clone(),
            vulkano::instance::InstanceCreateInfo {
                flags: vulkano::instance::InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )
        .expect("instance supporting required extensions available");

        let device_extensions = vulkano::device::DeviceExtensions {
            khr_swapchain: true,
            ..vulkano::device::DeviceExtensions::empty()
        };

        let surface =
            unsafe { vulkano::swapchain::Surface::from_window_ref(instance.clone(), &window) }
                .unwrap();

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags
                            .intersects(vulkano::device::QueueFlags::GRAPHICS)
                            && p.surface_support(i as u32, &surface).unwrap_or(false)
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                vulkano::device::physical::PhysicalDeviceType::DiscreteGpu => 0,
                vulkano::device::physical::PhysicalDeviceType::IntegratedGpu => 1,
                vulkano::device::physical::PhysicalDeviceType::VirtualGpu => 2,
                vulkano::device::physical::PhysicalDeviceType::Cpu => 3,
                vulkano::device::physical::PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .expect("suitable physical device available");

        let (device, mut queues) = vulkano::device::Device::new(
            physical_device,
            vulkano::device::DeviceCreateInfo {
                enabled_extensions: device_extensions,
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .expect("device initializes");

        let queue = queues.next().unwrap();

        let (swapchain, _images) = {
            let surface_capabilities = device
                .physical_device()
                .surface_capabilities(&surface, Default::default())
                .unwrap();

            let (image_format, _) = device
                .physical_device()
                .surface_formats(&surface, Default::default())
                .unwrap()[0];

            vulkano::swapchain::Swapchain::new(
                device.clone(),
                surface,
                vulkano::swapchain::SwapchainCreateInfo {
                    min_image_count: surface_capabilities.min_image_count.max(2),
                    image_extent: [width, height],
                    image_usage: vulkano::image::ImageUsage::COLOR_ATTACHMENT,
                    image_format,
                    present_mode: vulkano::swapchain::PresentMode::Fifo,
                    composite_alpha: surface_capabilities
                        .supported_composite_alpha
                        .into_iter()
                        .next()
                        .unwrap(),
                    ..Default::default()
                },
            )
        }
        .unwrap();

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    format: swapchain.image_format(),
                    samples: 1,
                    load_op: DontCare,
                    store_op: Store,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {},
            }
        )
        .unwrap();

        let framebuffers = vec![];

        let swapchain_is_valid = false;

        let last_render = Some(vulkano::sync::now(device.clone()).boxed());

        let gr_context = unsafe {
            let get_proc = |gpo| {
                let get_device_proc_addr = instance.fns().v1_0.get_device_proc_addr;

                match gpo {
                    vk::GetProcOf::Instance(instance, name) => {
                        let vk_instance = ash::vk::Instance::from_raw(instance as _);
                        library.get_instance_proc_addr(vk_instance, name)
                    }
                    vk::GetProcOf::Device(device, name) => {
                        let vk_device = ash::vk::Device::from_raw(device as _);
                        get_device_proc_addr(vk_device, name)
                    }
                }
                .map(|f| f as _)
                .unwrap()
            };

            direct_contexts::make_vulkan(
                &vk::BackendContext::new(
                    instance.handle().as_raw() as _,
                    device.physical_device().handle().as_raw() as _,
                    device.handle().as_raw() as _,
                    (
                        queue.handle().as_raw() as _,
                        queue.queue_family_index() as usize,
                    ),
                    &get_proc,
                ),
                None,
            )
        }
        .unwrap();

        VulkanBackend {
            gr_context,
            queue,
            swapchain,
            framebuffers,
            render_pass,
            last_render,
            swapchain_is_valid,
            prepared: None,
            window_size: [width, height],
        }
    }

    fn prepare_swapchain(&mut self) {
        if self.swapchain_is_valid {
            return;
        }

        let (new_swapchain, new_images) = self
            .swapchain
            .recreate(SwapchainCreateInfo {
                image_extent: self.window_size.clone(),
                ..self.swapchain.create_info()
            })
            .unwrap();

        self.swapchain = new_swapchain;

        self.framebuffers = new_images
            .iter()
            .map(|image| {
                let view = ImageView::new_default(image.clone()).unwrap();

                Framebuffer::new(
                    self.render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![view],
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();

        self.swapchain_is_valid = true;
    }

    fn get_next_frame(&mut self) -> Option<(u32, SwapchainAcquireFuture)> {
        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(self.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(it) => it,
                Err(VulkanError::OutOfDate) => {
                    self.swapchain_is_valid = false;
                    return None;
                }
                Err(e) => panic!("failed to caquire next image: {e:?}"),
            };

        if suboptimal {
            self.swapchain_is_valid = false;
        }

        if self.swapchain_is_valid {
            Some((image_index, acquire_future))
        } else {
            None
        }
    }

    fn create_surface_for_framebuffer(&mut self, framebuffer: Arc<Framebuffer>) -> Surface {
        let [width, height] = framebuffer.extent();
        let image_access = &framebuffer.attachments()[0];
        let image_object = image_access.image().handle().as_raw();

        let format = image_access.format();

        let (vk_format, color_type) = match format {
            vulkano::format::Format::B8G8R8A8_UNORM => {
                (vk::Format::B8G8R8A8_UNORM, ColorType::BGRA8888)
            }
            _ => panic!("Unsupported color format: {format:?}"),
        };

        let alloc = vk::Alloc::default();
        let image_info = &unsafe {
            vk::ImageInfo::new(
                image_object as _,
                alloc,
                vk::ImageTiling::OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk_format,
                1,
                None,
                None,
                None,
                None,
            )
        };

        let render_target =
            &backend_render_targets::make_vk((width as i32, height as i32), image_info);

        surfaces::wrap_backend_render_target(
            &mut self.gr_context,
            render_target,
            SurfaceOrigin::TopLeft,
            color_type,
            None,
            None,
        )
        .unwrap()
    }
}

impl SkiaBackend for VulkanBackend {
    fn set_size(&mut self, width: u32, height: u32) {
        self.window_size = [width, height];
        self.swapchain_is_valid = false;
    }

    fn prepare(&mut self) -> Option<Surface> {
        if let Some(last_render) = self.last_render.as_mut() {
            last_render.cleanup_finished();
        }

        let next_frame = self.get_next_frame().or_else(|| {
            self.prepare_swapchain();
            self.get_next_frame()
        });

        if let Some((image_index, acquire_future)) = next_frame {
            self.prepared = Some((image_index, acquire_future));
            return Some(
                self.create_surface_for_framebuffer(
                    self.framebuffers[image_index as usize].clone(),
                ),
            );
        } else {
            None
        }
    }

    // In vulkan implementation we do not reuse surface so we just drop it straight away
    fn flush(&mut self, _: Surface) {
        self.gr_context.flush_and_submit();

        let (image_index, acquire_future) = self.prepared.take().unwrap();

        self.last_render = self
            .last_render
            .take()
            .unwrap()
            .join(acquire_future)
            .then_swapchain_present(
                self.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(self.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush()
            .map(|f| Box::new(f) as _)
            .ok();
    }
}
