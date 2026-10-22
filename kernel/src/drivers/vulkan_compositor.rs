use crate::serial_println;
/// Vulkan Compositing Backend
///
/// GPU-accelerated compositing using Vulkan compute shaders,
/// texture atlas, layer blending, vsync.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct VulkanSurface {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub texture_handle: u64,
    pub z_order: i32,
    pub opacity: f32,
}

pub struct VulkanCompositor {
    pub surfaces: Vec<VulkanSurface>,
    pub screen_width: u32,
    pub screen_height: u32,
    pub vsync: bool,
    pub gpu_available: bool,
    pub frame_count: u64,
    pub next_surface_id: u32,
}

lazy_static::lazy_static! {
    static ref COMPOSITOR: Mutex<VulkanCompositor> = Mutex::new(VulkanCompositor {
        surfaces: Vec::new(),
        screen_width: 1920,
        screen_height: 1080,
        vsync: true,
        gpu_available: false,
        frame_count: 0,
        next_surface_id: 1,
    });
}

impl VulkanCompositor {
    pub fn create_surface(&mut self, w: u32, h: u32) -> u32 {
        let id = self.next_surface_id;
        self.next_surface_id += 1;
        self.surfaces.push(VulkanSurface {
            id,
            width: w,
            height: h,
            texture_handle: 0,
            z_order: self.surfaces.len() as i32,
            opacity: 1.0,
        });
        serial_println!("[VULKAN_COMP] Surface {} created ({}x{})", id, w, h);
        id
    }

    pub fn destroy_surface(&mut self, id: u32) {
        self.surfaces.retain(|s| s.id != id);
    }

    pub fn composite_frame(&mut self) {
        // Sort by z-order
        self.surfaces.sort_by_key(|s| s.z_order);
        // Would dispatch Vulkan compute shader to blend layers
        self.frame_count += 1;
    }

    pub fn set_opacity(&mut self, id: u32, opacity: f32) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.id == id) {
            s.opacity = opacity.clamp(0.0, 1.0);
        }
    }

    pub fn set_z_order(&mut self, id: u32, z: i32) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.id == id) {
            s.z_order = z;
        }
    }

    pub fn fps(&self) -> u64 {
        self.frame_count
    } // would track per-second
}

pub fn init() {
    serial_println!("[VULKAN_COMP] Vulkan compositor backend initialized");
}
