/// ResizeDirection — winit-inspired resize direction with cursor mapping
///
/// Provides a clean `ResizeDirection` enum that maps directly to cursor types,
/// replacing ad-hoc `ResizeEdge` ↔ `CursorType` matching scattered across the codebase.
///
/// Adapted from `winit/winit-core/src/window.rs`: `ResizeDirection`.
use super::desktop::CursorType;

/// Defines the direction that a window resize will be performed.
///
/// Each direction maps cleanly to a cursor icon, making resize logic
/// self-documenting: `ResizeDirection::SouthEast.cursor()` → `CursorType::ResizeSouthEast`.
///
/// Adapted from `winit::window::ResizeDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ResizeDirection {
    /// Resize from the east (right) edge →
    East,
    /// Resize from the north (top) edge ↑
    North,
    /// Resize from the north-east corner ↗
    NorthEast,
    /// Resize from the north-west corner ↖
    NorthWest,
    /// Resize from the south (bottom) edge ↓
    South,
    /// Resize from the south-east corner ↘
    SouthEast,
    /// Resize from the south-west corner ↙
    SouthWest,
    /// Resize from the west (left) edge ←
    West,
}

impl ResizeDirection {
    /// Get the cursor type for this resize direction.
    ///
    /// This is the `Into<CursorIcon>` mapping from winit, adapted for KnoxOS's
    /// `CursorType` enum.
    pub fn cursor(self) -> CursorType {
        match self {
            ResizeDirection::East => CursorType::ResizeEast,
            ResizeDirection::North => CursorType::ResizeNorth,
            ResizeDirection::NorthEast => CursorType::ResizeNorthEast,
            ResizeDirection::NorthWest => CursorType::ResizeNorthWest,
            ResizeDirection::South => CursorType::ResizeSouth,
            ResizeDirection::SouthEast => CursorType::ResizeSouthEast,
            ResizeDirection::SouthWest => CursorType::ResizeSouthWest,
            ResizeDirection::West => CursorType::ResizeWest,
        }
    }

    /// Get whether this direction involves horizontal resizing.
    #[inline]
    pub fn is_horizontal(self) -> bool {
        matches!(
            self,
            Self::East
                | Self::West
                | Self::NorthEast
                | Self::NorthWest
                | Self::SouthEast
                | Self::SouthWest
        )
    }

    /// Get whether this direction involves vertical resizing.
    #[inline]
    pub fn is_vertical(self) -> bool {
        matches!(
            self,
            Self::North
                | Self::South
                | Self::NorthEast
                | Self::NorthWest
                | Self::SouthEast
                | Self::SouthWest
        )
    }

    /// Get whether this is a corner resize (diagonal).
    #[inline]
    pub fn is_corner(self) -> bool {
        matches!(
            self,
            Self::NorthEast | Self::NorthWest | Self::SouthEast | Self::SouthWest
        )
    }

    /// Convert from the existing `ResizeEdge` enum in `window.rs`.
    /// Returns `None` for `ResizeEdge::None`.
    pub fn from_resize_edge(edge: super::window::ResizeEdge) -> Option<Self> {
        use super::window::ResizeEdge;
        match edge {
            ResizeEdge::None => None,
            ResizeEdge::Top => Some(ResizeDirection::North),
            ResizeEdge::Bottom => Some(ResizeDirection::South),
            ResizeEdge::Left => Some(ResizeDirection::West),
            ResizeEdge::Right => Some(ResizeDirection::East),
            ResizeEdge::TopLeft => Some(ResizeDirection::NorthWest),
            ResizeEdge::TopRight => Some(ResizeDirection::NorthEast),
            ResizeEdge::BottomLeft => Some(ResizeDirection::SouthWest),
            ResizeEdge::BottomRight => Some(ResizeDirection::SouthEast),
        }
    }

    /// Convert back to the `ResizeEdge` enum.
    pub fn to_resize_edge(self) -> super::window::ResizeEdge {
        use super::window::ResizeEdge;
        match self {
            ResizeDirection::East => ResizeEdge::Right,
            ResizeDirection::North => ResizeEdge::Top,
            ResizeDirection::NorthEast => ResizeEdge::TopRight,
            ResizeDirection::NorthWest => ResizeEdge::TopLeft,
            ResizeDirection::South => ResizeEdge::Bottom,
            ResizeDirection::SouthEast => ResizeEdge::BottomRight,
            ResizeDirection::SouthWest => ResizeEdge::BottomLeft,
            ResizeDirection::West => ResizeEdge::Left,
        }
    }
}

/// Convenience: get the cursor type for any `ResizeEdge`, returning `Default` for `None`.
pub fn cursor_for_edge(edge: super::window::ResizeEdge) -> CursorType {
    ResizeDirection::from_resize_edge(edge)
        .map(|d| d.cursor())
        .unwrap_or(CursorType::Default)
}
