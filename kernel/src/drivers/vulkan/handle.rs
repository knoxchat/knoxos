use core::sync::atomic::{AtomicU64, Ordering};

/// Type-safe opaque Vulkan handles.
macro_rules! vk_handle {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);
        impl $name {
            pub const NULL: Self = Self(0);
            pub fn is_null(&self) -> bool {
                self.0 == 0
            }
        }
    };
}

vk_handle!(VkInstance);
vk_handle!(VkPhysicalDevice);
vk_handle!(VkDevice);
vk_handle!(VkQueue);
vk_handle!(VkCommandPool);
vk_handle!(VkCommandBuffer);
vk_handle!(VkRenderPass);
vk_handle!(VkFramebuffer);
vk_handle!(VkPipeline);
vk_handle!(VkPipelineLayout);
vk_handle!(VkShaderModule);
vk_handle!(VkDescriptorSetLayout);
vk_handle!(VkDescriptorPool);
vk_handle!(VkDescriptorSet);
vk_handle!(VkBuffer);
vk_handle!(VkImage);
vk_handle!(VkImageView);
vk_handle!(VkSampler);
vk_handle!(VkDeviceMemory);
vk_handle!(VkFence);
vk_handle!(VkSemaphore);
vk_handle!(VkSwapchain);
vk_handle!(VkSurface);

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn alloc_handle() -> u64 {
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}
