use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Syntax Highlighting Code Editor Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxLanguage {
    PlainText,
    Rust,
    C,
    Python,
    JavaScript,
    Shell,
    Toml,
    Json,
    Markdown,
}

/// Syntax-highlighted code editor widget
pub struct CodeEditor {
    pub rect: Rect,
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub language: SyntaxLanguage,
    pub show_line_numbers: bool,
    pub tab_size: u8,
    pub bg_color: Pixel,
    pub gutter_color: Pixel,
}

impl CodeEditor {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            lines: alloc::vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            language: SyntaxLanguage::PlainText,
            show_line_numbers: true,
            tab_size: 4,
            bg_color: Pixel::new(18, 18, 28, 255),
            gutter_color: Pixel::new(30, 30, 42, 255),
        }
    }

    pub fn set_content(&mut self, text: &str) {
        self.lines = text.lines().map(String::from).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll_y = 0;
    }

    pub fn content(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }
        out
    }

    pub fn detect_language(&mut self, filename: &str) {
        self.language = if filename.ends_with(".rs") {
            SyntaxLanguage::Rust
        } else if filename.ends_with(".c") || filename.ends_with(".h") {
            SyntaxLanguage::C
        } else if filename.ends_with(".py") {
            SyntaxLanguage::Python
        } else if filename.ends_with(".js") || filename.ends_with(".ts") {
            SyntaxLanguage::JavaScript
        } else if filename.ends_with(".sh") {
            SyntaxLanguage::Shell
        } else if filename.ends_with(".toml") {
            SyntaxLanguage::Toml
        } else if filename.ends_with(".json") {
            SyntaxLanguage::Json
        } else if filename.ends_with(".md") {
            SyntaxLanguage::Markdown
        } else {
            SyntaxLanguage::PlainText
        };
    }

    /// Get syntax color for a token
    fn token_color(&self, token: &str, in_string: bool, in_comment: bool) -> Pixel {
        if in_comment {
            return Pixel::rgb(106, 135, 89);
        }
        if in_string {
            return Pixel::rgb(206, 145, 120);
        }

        let keywords = match self.language {
            SyntaxLanguage::Rust => &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod", "if", "else",
                "for", "while", "loop", "match", "return", "self", "Self", "crate", "super",
                "true", "false", "const", "static", "unsafe", "async", "await", "trait", "where",
                "type",
            ] as &[&str],
            SyntaxLanguage::C => &[
                "int", "void", "char", "float", "double", "if", "else", "for", "while", "return",
                "struct", "typedef", "enum", "const", "static", "sizeof", "switch", "case",
                "break", "continue",
            ],
            SyntaxLanguage::Python => &[
                "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from",
                "as", "with", "try", "except", "raise", "True", "False", "None", "and", "or",
                "not", "in", "is", "lambda", "yield", "async", "await",
            ],
            SyntaxLanguage::JavaScript => &[
                "function",
                "const",
                "let",
                "var",
                "if",
                "else",
                "for",
                "while",
                "return",
                "class",
                "new",
                "this",
                "true",
                "false",
                "null",
                "undefined",
                "async",
                "await",
                "import",
                "export",
            ],
            _ => &[],
        };

        if keywords.contains(&token) {
            return Pixel::rgb(86, 156, 214); // Blue for keywords
        }

        // Numbers
        if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Pixel::rgb(181, 206, 168); // Green for numbers
        }

        Pixel::rgb(212, 212, 212) // Default foreground
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        fb.fill_rect(self.rect, self.bg_color);
        let gutter_w = if self.show_line_numbers { 48i32 } else { 0 };
        let line_h = 16;
        let visible_lines = (self.rect.height as usize / line_h as usize).max(1);

        // Gutter background
        if self.show_line_numbers {
            fb.fill_rect(
                Rect::new(self.rect.x, self.rect.y, gutter_w as u32, self.rect.height),
                self.gutter_color,
            );
        }

        for i in 0..visible_lines {
            let line_idx = self.scroll_y + i;
            if line_idx >= self.lines.len() {
                break;
            }
            let y = self.rect.y + (i as i32 * line_h);

            // Current line highlight
            if line_idx == self.cursor_line {
                fb.fill_rect(
                    Rect::new(
                        self.rect.x + gutter_w,
                        y,
                        self.rect.width - gutter_w as u32,
                        line_h as u32,
                    ),
                    Pixel::new(40, 40, 60, 128),
                );
            }

            // Line number
            if self.show_line_numbers {
                let num = alloc::format!("{:>4}", line_idx + 1);
                let num_color = if line_idx == self.cursor_line {
                    Pixel::rgb(200, 200, 220)
                } else {
                    Pixel::rgb(90, 90, 110)
                };
                fonts::draw_string_compact(fb, self.rect.x + 4, y + 2, &num, num_color, 1);
            }

            // Line text with syntax highlighting (simplified per-word coloring)
            let line = &self.lines[line_idx];
            let mut x = self.rect.x + gutter_w + 4;
            let mut in_comment = false;
            let mut in_string = false;

            for ch in line.chars() {
                if ch == '/' && line.contains("//") {
                    in_comment = true;
                }
                if ch == '#'
                    && (self.language == SyntaxLanguage::Python
                        || self.language == SyntaxLanguage::Shell)
                {
                    in_comment = true;
                }
                if ch == '"' || ch == '\'' {
                    in_string = !in_string;
                }

                let color = if in_comment {
                    Pixel::rgb(106, 135, 89)
                } else if in_string {
                    Pixel::rgb(206, 145, 120)
                } else if ch.is_ascii_digit() {
                    Pixel::rgb(181, 206, 168)
                } else if "{}()[]<>".contains(ch) {
                    Pixel::rgb(218, 218, 120)
                } else if "+-*/%=!&|^~".contains(ch) {
                    Pixel::rgb(180, 180, 180)
                } else {
                    Pixel::rgb(212, 212, 212)
                };

                fonts::draw_char(fb, x, y + 2, ch, color, 1);
                x += 8;
            }

            // Cursor
            if line_idx == self.cursor_line {
                let cx = self.rect.x + gutter_w + 4 + self.cursor_col as i32 * 8;
                fb.fill_rect(Rect::new(cx, y + 2, 2, 13), Pixel::rgb(200, 200, 255));
            }
        }
    }
}
