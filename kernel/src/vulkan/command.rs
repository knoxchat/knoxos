use alloc::vec::Vec;

use super::enums::{VkFilter, VkImageLayout, VkIndexType, VkPipelineBindPoint};
use super::handle::{
    VkBuffer, VkDescriptorSet, VkFramebuffer, VkImage, VkPipeline, VkPipelineLayout, VkRenderPass,
};

/// Recorded command-buffer operations.
#[derive(Debug, Clone)]
pub enum VkCommand {
    BeginRenderPass {
        render_pass: VkRenderPass,
        framebuffer: VkFramebuffer,
        clear_values: Vec<[f32; 4]>,
    },
    EndRenderPass,
    BindPipeline {
        bind_point: VkPipelineBindPoint,
        pipeline: VkPipeline,
    },
    BindDescriptorSets {
        bind_point: VkPipelineBindPoint,
        layout: VkPipelineLayout,
        sets: Vec<VkDescriptorSet>,
    },
    BindVertexBuffers {
        first_binding: u32,
        buffers: Vec<VkBuffer>,
        offsets: Vec<u64>,
    },
    BindIndexBuffer {
        buffer: VkBuffer,
        offset: u64,
        index_type: VkIndexType,
    },
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    },
    Dispatch {
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    },
    CopyBuffer {
        src: VkBuffer,
        dst: VkBuffer,
        size: u64,
        src_offset: u64,
        dst_offset: u64,
    },
    CopyBufferToImage {
        src_buffer: VkBuffer,
        dst_image: VkImage,
        layout: VkImageLayout,
    },
    CopyImageToBuffer {
        src_image: VkImage,
        dst_buffer: VkBuffer,
        layout: VkImageLayout,
    },
    PipelineBarrier {
        src_stage: u32,
        dst_stage: u32,
    },
    PushConstants {
        layout: VkPipelineLayout,
        stage_flags: u32,
        offset: u32,
        data: Vec<u8>,
    },
    SetViewport {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetScissor {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    BlitImage {
        src: VkImage,
        dst: VkImage,
        filter: VkFilter,
    },
    ClearColorImage {
        image: VkImage,
        color: [f32; 4],
    },
    ClearDepthStencilImage {
        image: VkImage,
        depth: f32,
        stencil: u32,
    },
}
