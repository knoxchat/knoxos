/// Terminal color theme (matches a modern dark terminal like Alacritty's default)
use crate::gui::framebuffer::Pixel;

pub struct TerminalTheme {
    pub background: Pixel,
    pub foreground: Pixel,
    pub cursor: Pixel,
    pub cursor_text: Pixel,
    pub selection_bg: Pixel,
    pub selection_fg: Pixel,
    pub suggestion_fg: Pixel,   // Fish autosuggestion ghost text
    pub error_fg: Pixel,        // Syntax highlight: invalid command
    pub command_fg: Pixel,      // Syntax highlight: valid command
    pub argument_fg: Pixel,     // Syntax highlight: arguments
    pub string_fg: Pixel,       // Syntax highlight: quoted strings
    pub operator_fg: Pixel,     // Syntax highlight: pipes, redirects
    pub variable_fg: Pixel,     // Syntax highlight: $VARIABLES
    pub comment_fg: Pixel,      // Syntax highlight: # comments
    pub path_fg: Pixel,         // Syntax highlight: valid paths
    pub search_match_bg: Pixel, // Search highlight background
    pub search_match_fg: Pixel, // Search highlight foreground
    pub scrollbar_bg: Pixel,
    pub scrollbar_thumb: Pixel,
    /// ANSI 16-color palette
    pub palette: [Pixel; 16],
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}

impl TerminalTheme {
    /// Tokyo Night theme (default)
    pub fn tokyo_night() -> Self {
        Self {
            background: Pixel::rgb(26, 27, 38),
            foreground: Pixel::rgb(192, 202, 218),
            cursor: Pixel::rgb(82, 139, 255),
            cursor_text: Pixel::rgb(26, 27, 38),
            selection_bg: Pixel::new(82, 139, 255, 100),
            selection_fg: Pixel::rgb(255, 255, 255),
            suggestion_fg: Pixel::rgb(88, 92, 108),
            error_fg: Pixel::rgb(255, 85, 85),
            command_fg: Pixel::rgb(130, 170, 255),
            argument_fg: Pixel::rgb(192, 202, 218),
            string_fg: Pixel::rgb(158, 206, 106),
            operator_fg: Pixel::rgb(187, 154, 247),
            variable_fg: Pixel::rgb(224, 175, 104),
            comment_fg: Pixel::rgb(88, 92, 108),
            path_fg: Pixel::rgb(115, 218, 202),
            search_match_bg: Pixel::rgb(224, 175, 104),
            search_match_fg: Pixel::rgb(26, 27, 38),
            scrollbar_bg: Pixel::rgb(36, 37, 48),
            scrollbar_thumb: Pixel::rgb(68, 71, 90),
            palette: [
                Pixel::rgb(26, 27, 38),
                Pixel::rgb(247, 118, 142),
                Pixel::rgb(158, 206, 106),
                Pixel::rgb(224, 175, 104),
                Pixel::rgb(122, 162, 247),
                Pixel::rgb(187, 154, 247),
                Pixel::rgb(125, 207, 255),
                Pixel::rgb(192, 202, 218),
                Pixel::rgb(88, 92, 108),
                Pixel::rgb(255, 117, 127),
                Pixel::rgb(182, 231, 130),
                Pixel::rgb(255, 199, 119),
                Pixel::rgb(130, 170, 255),
                Pixel::rgb(200, 170, 255),
                Pixel::rgb(141, 222, 255),
                Pixel::rgb(219, 226, 239),
            ],
        }
    }

    /// Solarized Dark theme
    pub fn solarized_dark() -> Self {
        Self {
            background: Pixel::rgb(0, 43, 54),
            foreground: Pixel::rgb(131, 148, 150),
            cursor: Pixel::rgb(211, 54, 130),
            cursor_text: Pixel::rgb(0, 43, 54),
            selection_bg: Pixel::new(7, 54, 66, 180),
            selection_fg: Pixel::rgb(238, 232, 213),
            suggestion_fg: Pixel::rgb(88, 110, 117),
            error_fg: Pixel::rgb(220, 50, 47),
            command_fg: Pixel::rgb(38, 139, 210),
            argument_fg: Pixel::rgb(131, 148, 150),
            string_fg: Pixel::rgb(42, 161, 152),
            operator_fg: Pixel::rgb(211, 54, 130),
            variable_fg: Pixel::rgb(181, 137, 0),
            comment_fg: Pixel::rgb(88, 110, 117),
            path_fg: Pixel::rgb(133, 153, 0),
            search_match_bg: Pixel::rgb(181, 137, 0),
            search_match_fg: Pixel::rgb(0, 43, 54),
            scrollbar_bg: Pixel::rgb(7, 54, 66),
            scrollbar_thumb: Pixel::rgb(88, 110, 117),
            palette: [
                Pixel::rgb(7, 54, 66),
                Pixel::rgb(220, 50, 47),
                Pixel::rgb(133, 153, 0),
                Pixel::rgb(181, 137, 0),
                Pixel::rgb(38, 139, 210),
                Pixel::rgb(211, 54, 130),
                Pixel::rgb(42, 161, 152),
                Pixel::rgb(238, 232, 213),
                Pixel::rgb(0, 43, 54),
                Pixel::rgb(203, 75, 22),
                Pixel::rgb(88, 110, 117),
                Pixel::rgb(101, 123, 131),
                Pixel::rgb(131, 148, 150),
                Pixel::rgb(108, 113, 196),
                Pixel::rgb(147, 161, 161),
                Pixel::rgb(253, 246, 227),
            ],
        }
    }

    /// Monokai theme
    pub fn monokai() -> Self {
        Self {
            background: Pixel::rgb(39, 40, 34),
            foreground: Pixel::rgb(248, 248, 242),
            cursor: Pixel::rgb(248, 248, 240),
            cursor_text: Pixel::rgb(39, 40, 34),
            selection_bg: Pixel::new(73, 72, 62, 180),
            selection_fg: Pixel::rgb(248, 248, 242),
            suggestion_fg: Pixel::rgb(117, 113, 94),
            error_fg: Pixel::rgb(249, 38, 114),
            command_fg: Pixel::rgb(166, 226, 46),
            argument_fg: Pixel::rgb(248, 248, 242),
            string_fg: Pixel::rgb(230, 219, 116),
            operator_fg: Pixel::rgb(249, 38, 114),
            variable_fg: Pixel::rgb(253, 151, 31),
            comment_fg: Pixel::rgb(117, 113, 94),
            path_fg: Pixel::rgb(102, 217, 239),
            search_match_bg: Pixel::rgb(230, 219, 116),
            search_match_fg: Pixel::rgb(39, 40, 34),
            scrollbar_bg: Pixel::rgb(49, 50, 44),
            scrollbar_thumb: Pixel::rgb(90, 90, 80),
            palette: [
                Pixel::rgb(39, 40, 34),
                Pixel::rgb(249, 38, 114),
                Pixel::rgb(166, 226, 46),
                Pixel::rgb(230, 219, 116),
                Pixel::rgb(102, 217, 239),
                Pixel::rgb(174, 129, 255),
                Pixel::rgb(161, 239, 228),
                Pixel::rgb(248, 248, 242),
                Pixel::rgb(117, 113, 94),
                Pixel::rgb(249, 38, 114),
                Pixel::rgb(166, 226, 46),
                Pixel::rgb(230, 219, 116),
                Pixel::rgb(102, 217, 239),
                Pixel::rgb(174, 129, 255),
                Pixel::rgb(161, 239, 228),
                Pixel::rgb(248, 248, 242),
            ],
        }
    }

    /// Dracula theme
    pub fn dracula() -> Self {
        Self {
            background: Pixel::rgb(40, 42, 54),
            foreground: Pixel::rgb(248, 248, 242),
            cursor: Pixel::rgb(248, 248, 242),
            cursor_text: Pixel::rgb(40, 42, 54),
            selection_bg: Pixel::new(68, 71, 90, 180),
            selection_fg: Pixel::rgb(248, 248, 242),
            suggestion_fg: Pixel::rgb(98, 114, 164),
            error_fg: Pixel::rgb(255, 85, 85),
            command_fg: Pixel::rgb(80, 250, 123),
            argument_fg: Pixel::rgb(248, 248, 242),
            string_fg: Pixel::rgb(241, 250, 140),
            operator_fg: Pixel::rgb(255, 121, 198),
            variable_fg: Pixel::rgb(189, 147, 249),
            comment_fg: Pixel::rgb(98, 114, 164),
            path_fg: Pixel::rgb(139, 233, 253),
            search_match_bg: Pixel::rgb(241, 250, 140),
            search_match_fg: Pixel::rgb(40, 42, 54),
            scrollbar_bg: Pixel::rgb(68, 71, 90),
            scrollbar_thumb: Pixel::rgb(98, 114, 164),
            palette: [
                Pixel::rgb(33, 34, 44),
                Pixel::rgb(255, 85, 85),
                Pixel::rgb(80, 250, 123),
                Pixel::rgb(241, 250, 140),
                Pixel::rgb(189, 147, 249),
                Pixel::rgb(255, 121, 198),
                Pixel::rgb(139, 233, 253),
                Pixel::rgb(248, 248, 242),
                Pixel::rgb(98, 114, 164),
                Pixel::rgb(255, 110, 110),
                Pixel::rgb(105, 255, 148),
                Pixel::rgb(255, 255, 165),
                Pixel::rgb(214, 172, 255),
                Pixel::rgb(255, 146, 223),
                Pixel::rgb(164, 255, 255),
                Pixel::rgb(255, 255, 255),
            ],
        }
    }

    /// Get a theme by name
    pub fn by_name(name: &str) -> Self {
        match name {
            "solarized" | "solarized-dark" => Self::solarized_dark(),
            "monokai" => Self::monokai(),
            "dracula" => Self::dracula(),
            _ => Self::tokyo_night(), // default
        }
    }
}
