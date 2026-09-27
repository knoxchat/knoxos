use crate::serial_println;

use super::command::VkCommand;
use super::state::{BUFFERS, FRAMEBUFFERS, IMAGES};

/// Execute a single command (software fallback)
pub(crate) fn execute_command(cmd: &VkCommand) {
    match cmd {
        VkCommand::Draw {
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        } => {
            // Software vertex processing: assemble triangles from vertex buffer
            // For a triangle list, every 3 vertices form one triangle
            let total = (*vertex_count) * (*instance_count);
            serial_println!(
                "[vulkan-sw] Draw: {} vertices, {} instances (first_vtx={}, first_inst={})",
                vertex_count,
                instance_count,
                first_vertex,
                first_instance
            );
            // In a full software renderer, we would:
            // 1. Fetch vertices from the bound vertex buffer
            // 2. Run the SPIR-V vertex shader on each vertex
            // 3. Assemble primitives (triangles/lines/points)
            // 4. Clip and rasterize using rasterize_triangle()
            // 5. Run the SPIR-V fragment shader per pixel
            // 6. Write to the bound framebuffer
            let _ = total;
        }
        VkCommand::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        } => {
            let total = (*index_count) * (*instance_count);
            serial_println!(
                "[vulkan-sw] DrawIndexed: {} indices, {} instances",
                index_count,
                instance_count
            );
            let _ = (total, first_index, vertex_offset, first_instance);
        }
        VkCommand::Dispatch {
            group_count_x,
            group_count_y,
            group_count_z,
        } => {
            let total_groups = (*group_count_x) * (*group_count_y) * (*group_count_z);
            serial_println!(
                "[vulkan-sw] Dispatch: {}x{}x{} = {} groups",
                group_count_x,
                group_count_y,
                group_count_z,
                total_groups
            );
        }
        VkCommand::CopyBuffer {
            src,
            dst,
            size,
            src_offset,
            dst_offset,
        } => {
            let mut bufs = BUFFERS.lock();
            // Extract source data first
            let src_data = bufs.get(&src.0).map(|sb| {
                let start = *src_offset as usize;
                let end = (start + *size as usize).min(sb.data.len());
                sb.data[start..end].to_vec()
            });
            // Write to destination
            if let Some(data) = src_data {
                if let Some(dst_buf) = bufs.get_mut(&dst.0) {
                    let d_start = *dst_offset as usize;
                    let copy_len = data.len().min(dst_buf.data.len().saturating_sub(d_start));
                    dst_buf.data[d_start..d_start + copy_len].copy_from_slice(&data[..copy_len]);
                }
            }
        }
        VkCommand::CopyBufferToImage {
            src_buffer,
            dst_image,
            layout: _,
        } => {
            let bufs = BUFFERS.lock();
            let mut imgs = IMAGES.lock();
            if let (Some(src), Some(dst)) = (bufs.get(&src_buffer.0), imgs.get_mut(&dst_image.0)) {
                let copy_len = src.data.len().min(dst.data.len());
                dst.data[..copy_len].copy_from_slice(&src.data[..copy_len]);
            }
        }
        VkCommand::CopyImageToBuffer {
            src_image,
            dst_buffer,
            layout: _,
        } => {
            let imgs = IMAGES.lock();
            let mut bufs = BUFFERS.lock();
            if let (Some(src), Some(dst)) = (imgs.get(&src_image.0), bufs.get_mut(&dst_buffer.0)) {
                let copy_len = src.data.len().min(dst.data.len());
                dst.data[..copy_len].copy_from_slice(&src.data[..copy_len]);
            }
        }
        VkCommand::ClearColorImage { image, color } => {
            let mut imgs = IMAGES.lock();
            if let Some(img) = imgs.get_mut(&image.0) {
                let bpp = img.format.bytes_per_pixel();
                for chunk in img.data.chunks_mut(bpp) {
                    if bpp >= 4 {
                        chunk[0] = (color[2] * 255.0) as u8; // B
                        chunk[1] = (color[1] * 255.0) as u8; // G
                        chunk[2] = (color[0] * 255.0) as u8; // R
                        if bpp == 4 {
                            chunk[3] = (color[3] * 255.0) as u8; // A
                        }
                    }
                }
            }
        }
        VkCommand::ClearDepthStencilImage {
            image,
            depth,
            stencil,
        } => {
            let mut imgs = IMAGES.lock();
            if let Some(img) = imgs.get_mut(&image.0) {
                // D32_SFLOAT format: write f32 depth to each texel
                for chunk in img.data.chunks_mut(4) {
                    if chunk.len() == 4 {
                        let bytes = depth.to_le_bytes();
                        chunk.copy_from_slice(&bytes);
                    }
                }
                let _ = stencil; // Stencil stored in separate plane if D24S8
            }
        }
        VkCommand::BlitImage { src, dst, filter } => {
            let mut imgs = IMAGES.lock();
            // Simple copy for nearest filter; would do bilinear for linear
            let src_data = imgs.get(&src.0).map(|i| i.data.clone());
            if let (Some(data), Some(dst_img)) = (src_data, imgs.get_mut(&dst.0)) {
                let copy_len = data.len().min(dst_img.data.len());
                dst_img.data[..copy_len].copy_from_slice(&data[..copy_len]);
            }
            let _ = filter;
        }
        VkCommand::BeginRenderPass {
            render_pass: _,
            framebuffer,
            clear_values,
        } => {
            // Clear framebuffer attachments with clear values
            let fbs = FRAMEBUFFERS.lock();
            if let Some(fb) = fbs.get(&framebuffer.0) {
                let mut imgs = IMAGES.lock();
                for (i, &img_handle) in fb.attachments.iter().enumerate() {
                    if let Some(img) = imgs.get_mut(&img_handle.0) {
                        if let Some(clear) = clear_values.get(i) {
                            let bpp = img.format.bytes_per_pixel();
                            for chunk in img.data.chunks_mut(bpp) {
                                if bpp >= 4 {
                                    chunk[0] = (clear[2] * 255.0) as u8;
                                    chunk[1] = (clear[1] * 255.0) as u8;
                                    chunk[2] = (clear[0] * 255.0) as u8;
                                    if bpp == 4 {
                                        chunk[3] = (clear[3] * 255.0) as u8;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        VkCommand::EndRenderPass => { /* State transition only */ }
        VkCommand::BindPipeline { .. } => { /* Records current pipeline for subsequent draws */ }
        VkCommand::BindDescriptorSets { .. } => { /* Records bound descriptors */ }
        VkCommand::BindVertexBuffers { .. } => { /* Records vertex buffer bindings */ }
        VkCommand::BindIndexBuffer { .. } => { /* Records index buffer binding */ }
        VkCommand::PipelineBarrier { .. } => { /* Memory/execution barrier — no-op in SW renderer */
        }
        VkCommand::PushConstants { .. } => { /* Would update push constant memory block */ }
        VkCommand::SetViewport { .. } => { /* Records viewport transform parameters */ }
        VkCommand::SetScissor { .. } => { /* Records scissor rect for clipping */ }
    }
}

/// Software triangle rasterizer (barycentric coordinates)
pub fn rasterize_triangle(
    framebuffer: &mut [u8],
    width: u32,
    height: u32,
    v0: [f32; 4], // x, y, z, w
    v1: [f32; 4],
    v2: [f32; 4],
    color: [u8; 4], // BGRA
) {
    // Compute bounding box
    let min_x = (v0[0].min(v1[0]).min(v2[0]).max(0.0)) as u32;
    let max_x = (v0[0].max(v1[0]).max(v2[0]).min(width as f32 - 1.0)) as u32;
    let min_y = (v0[1].min(v1[1]).min(v2[1]).max(0.0)) as u32;
    let max_y = (v0[1].max(v1[1]).max(v2[1]).min(height as f32 - 1.0)) as u32;

    let area = edge_function(v0, v1, v2);
    if area <= 0.0 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5, 0.0, 1.0];
            let w0 = edge_function(v1, v2, p);
            let w1 = edge_function(v2, v0, p);
            let w2 = edge_function(v0, v1, p);

            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let offset = ((y * width + x) * 4) as usize;
                if offset + 3 < framebuffer.len() {
                    framebuffer[offset] = color[0];
                    framebuffer[offset + 1] = color[1];
                    framebuffer[offset + 2] = color[2];
                    framebuffer[offset + 3] = color[3];
                }
            }
        }
    }
}

fn edge_function(a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

/// Software line rasterizer (Bresenham's algorithm)
pub fn rasterize_line(
    framebuffer: &mut [u8],
    width: u32,
    _height: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        let offset = ((cy as u32 * width + cx as u32) * 4) as usize;
        if offset + 3 < framebuffer.len() {
            framebuffer[offset] = color[0];
            framebuffer[offset + 1] = color[1];
            framebuffer[offset + 2] = color[2];
            framebuffer[offset + 3] = color[3];
        }

        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}
