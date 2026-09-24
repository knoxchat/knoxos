use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Chart / Graph Widget (line, bar, pie)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
    Pie,
}

/// A data series for the chart
#[derive(Debug, Clone)]
pub struct ChartSeries {
    pub label: String,
    pub color: Pixel,
    pub values: Vec<f32>,
}

/// Chart widget
pub struct Chart {
    pub rect: Rect,
    pub chart_type: ChartType,
    pub title: String,
    pub series: Vec<ChartSeries>,
    pub x_labels: Vec<String>,
    pub y_min: f32,
    pub y_max: f32,
    pub bg_color: Pixel,
    pub grid_color: Pixel,
    pub text_color: Pixel,
}

impl Chart {
    pub fn new(x: i32, y: i32, w: u32, h: u32, chart_type: ChartType, title: &str) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            chart_type,
            title: String::from(title),
            series: Vec::new(),
            x_labels: Vec::new(),
            y_min: 0.0,
            y_max: 100.0,
            bg_color: Pixel::new(20, 20, 30, 240),
            grid_color: Pixel::new(60, 60, 80, 128),
            text_color: Pixel::rgb(180, 180, 200),
        }
    }

    pub fn add_series(&mut self, label: &str, color: Pixel, values: Vec<f32>) {
        self.series.push(ChartSeries {
            label: String::from(label),
            color,
            values,
        });
        // Auto-scale
        for s in &self.series {
            for &v in &s.values {
                if v > self.y_max {
                    self.y_max = v;
                }
                if v < self.y_min {
                    self.y_min = v;
                }
            }
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let r = self.rect;
        fb.fill_rounded_rect_aa(r, self.bg_color, 8);

        // Title
        fonts::draw_string_compact(fb, r.x + 8, r.y + 4, &self.title, self.text_color, 1);

        let margin = 40i32;
        let plot_x = r.x + margin;
        let plot_y = r.y + 24;
        let plot_w = (r.width as i32 - margin - 8).max(1);
        let plot_h = (r.height as i32 - 32 - 8).max(1);

        // Grid lines
        for i in 0..=4 {
            let gy = plot_y + plot_h - (plot_h * i / 4);
            for gx in (plot_x..plot_x + plot_w).step_by(3) {
                fb.set_pixel(gx as usize, gy as usize, self.grid_color);
            }
        }

        let range = (self.y_max - self.y_min).max(1.0);

        match self.chart_type {
            ChartType::Line => {
                for series in &self.series {
                    let n = series.values.len().max(1);
                    for i in 1..n {
                        let x0 = plot_x + (plot_w * (i - 1) as i32 / n.max(1) as i32);
                        let x1 = plot_x + (plot_w * i as i32 / n.max(1) as i32);
                        let y0 = plot_y + plot_h
                            - ((series.values[i - 1] - self.y_min) / range * plot_h as f32) as i32;
                        let y1 = plot_y + plot_h
                            - ((series.values[i] - self.y_min) / range * plot_h as f32) as i32;
                        fb.draw_line_aa(x0, y0, x1, y1, series.color);
                    }
                }
            }
            ChartType::Bar => {
                if let Some(series) = self.series.first() {
                    let n = series.values.len().max(1);
                    let bar_w = (plot_w / n as i32 - 2).max(1);
                    for (i, &v) in series.values.iter().enumerate() {
                        let bx = plot_x + (plot_w * i as i32 / n as i32) + 1;
                        let bh = ((v - self.y_min) / range * plot_h as f32) as i32;
                        let by = plot_y + plot_h - bh;
                        fb.fill_rect(Rect::new(bx, by, bar_w as u32, bh as u32), series.color);
                    }
                }
            }
            ChartType::Pie => {
                if let Some(series) = self.series.first() {
                    let cx = r.x + r.width as i32 / 2;
                    let cy = plot_y + plot_h / 2;
                    let radius = plot_h.min(plot_w) / 2 - 4;
                    let total: f32 = series.values.iter().sum::<f32>().max(0.001);
                    let pie_colors = [
                        Pixel::rgb(66, 133, 244),
                        Pixel::rgb(234, 67, 53),
                        Pixel::rgb(251, 188, 4),
                        Pixel::rgb(52, 168, 83),
                        Pixel::rgb(171, 71, 188),
                        Pixel::rgb(255, 112, 67),
                    ];
                    // Simple pie by filling circle segments
                    let mut angle_start = 0.0f32;
                    for (i, &v) in series.values.iter().enumerate() {
                        let sweep = v / total * 360.0;
                        let color = pie_colors[i % pie_colors.len()];
                        // Fill arc by iterating pixels in bounding box
                        for py in (cy - radius)..=(cy + radius) {
                            for px in (cx - radius)..=(cx + radius) {
                                let dx = (px - cx) as f32;
                                let dy = (py - cy) as f32;
                                if dx * dx + dy * dy <= (radius * radius) as f32 {
                                    let mut angle =
                                        libm::atan2f(dy, dx) * 180.0 / core::f32::consts::PI;
                                    if angle < 0.0 {
                                        angle += 360.0;
                                    }
                                    if angle >= angle_start && angle < angle_start + sweep {
                                        fb.blend_pixel(px as usize, py as usize, color);
                                    }
                                }
                            }
                        }
                        angle_start += sweep;
                    }
                }
            }
        }
    }
}
