// SPDX-License-Identifier: MIT
//! Glyph / icon sprite atlas (GPUI `SpriteAtlas` role).
//!
//! Each tile is an 8-bit coverage bitmap. Texture ids are monotonic; eviction
//! drops the oldest tile so the map stays bounded (D5.4).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::geometry::{Bgra, Bounds};
use crate::scene::AtlasTile;

enum TilePixels {
    Alpha(Vec<u8>),
    Bgra(Vec<u8>),
}

struct Tile {
    width: u32,
    height: u32,
    pixels: TilePixels,
}

/// CPU atlas of glyph (alpha) and image (BGRA) tiles.
pub struct SpriteAtlas {
    next_id: u32,
    max_tiles: usize,
    tiles: BTreeMap<u32, Tile>,
    intern: BTreeMap<u64, u32>,
}

impl SpriteAtlas {
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            max_tiles: 8192,
            tiles: BTreeMap::new(),
            intern: BTreeMap::new(),
        }
    }

    pub fn with_max_tiles(max_tiles: usize) -> Self {
        Self {
            next_id: 1,
            max_tiles: max_tiles.max(1),
            tiles: BTreeMap::new(),
            intern: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn evict_if_needed(&mut self) {
        while self.tiles.len() >= self.max_tiles {
            // Drop one-shot tiles before interned glyphs/icons so a busy
            // gallery frame cannot punch holes in already-painted letters.
            let interned = &self.intern;
            let victim = self
                .tiles
                .keys()
                .copied()
                .find(|id| !interned.values().any(|keep| keep == id))
                .or_else(|| self.tiles.keys().next().copied());
            let Some(oldest) = victim else {
                break;
            };
            self.tiles.remove(&oldest);
            self.intern.retain(|_, id| *id != oldest);
        }
    }

    fn insert_tile(&mut self, width: u32, height: u32, pixels: TilePixels) -> AtlasTile {
        self.evict_if_needed();
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.tiles.insert(
            id,
            Tile {
                width,
                height,
                pixels,
            },
        );
        AtlasTile::new(id, Bounds::from_xywh(0, 0, width as i32, height as i32))
    }

    /// Insert an 8-bit coverage bitmap. Evicts oldest tiles if over cap.
    pub fn alloc_alpha(&mut self, width: u32, height: u32, alpha: Vec<u8>) -> AtlasTile {
        self.insert_tile(width, height, TilePixels::Alpha(alpha))
    }

    /// Insert packed BGRA8888 pixels (`width * height * 4` bytes).
    pub fn alloc_bgra(&mut self, width: u32, height: u32, pixels: Vec<u8>) -> AtlasTile {
        self.insert_tile(width, height, TilePixels::Bgra(pixels))
    }

    fn intern_lookup(&self, key: u64) -> Option<AtlasTile> {
        if key == 0 {
            return None;
        }
        let id = *self.intern.get(&key)?;
        let tile = self.tiles.get(&id)?;
        Some(AtlasTile::new(
            id,
            Bounds::from_xywh(0, 0, tile.width as i32, tile.height as i32),
        ))
    }

    /// Reuse a BGRA tile for `key` across frames. `key == 0` always allocates.
    pub fn intern_bgra(&mut self, key: u64, width: u32, height: u32, pixels: Vec<u8>) -> AtlasTile {
        if let Some(tile) = self.intern_lookup(key) {
            return tile;
        }
        let tile = self.alloc_bgra(width, height, pixels);
        if key != 0 {
            self.intern.insert(key, tile.texture_id);
        }
        tile
    }

    /// Reuse an alpha tile for `key` across frames. `key == 0` always allocates.
    pub fn intern_alpha(&mut self, key: u64, width: u32, height: u32, alpha: Vec<u8>) -> AtlasTile {
        if let Some(tile) = self.intern_lookup(key) {
            if tile.bounds.width() == width as i32 && tile.bounds.height() == height as i32 {
                return tile;
            }
            if let Some(id) = self.intern.remove(&key) {
                self.tiles.remove(&id);
            }
        }
        let tile = self.alloc_alpha(width, height, alpha);
        if key != 0 {
            self.intern.insert(key, tile.texture_id);
        }
        tile
    }

    pub fn tile_size(&self, tile: AtlasTile) -> Option<(u32, u32)> {
        self.tiles
            .get(&tile.texture_id)
            .map(|t| (t.width, t.height))
    }

    pub fn coverage(&self, tile: AtlasTile, x: u32, y: u32) -> u8 {
        let Some(t) = self.tiles.get(&tile.texture_id) else {
            return 0;
        };
        if x >= t.width || y >= t.height {
            return 0;
        }
        let i = (y * t.width + x) as usize;
        match &t.pixels {
            TilePixels::Alpha(alpha) => alpha.get(i).copied().unwrap_or(0),
            TilePixels::Bgra(px) => {
                let o = i.saturating_mul(4);
                px.get(o + 3).copied().unwrap_or(0)
            }
        }
    }

    /// Sample a BGRA tile. Alpha tiles become white with that coverage.
    pub fn bgra(&self, tile: AtlasTile, x: u32, y: u32) -> Bgra {
        let Some(t) = self.tiles.get(&tile.texture_id) else {
            return Bgra::TRANSPARENT;
        };
        if x >= t.width || y >= t.height {
            return Bgra::TRANSPARENT;
        }
        let i = (y * t.width + x) as usize;
        match &t.pixels {
            TilePixels::Alpha(alpha) => {
                let a = alpha.get(i).copied().unwrap_or(0);
                Bgra::rgba(255, 255, 255, a)
            }
            TilePixels::Bgra(px) => {
                let o = i.saturating_mul(4);
                if o + 3 >= px.len() {
                    return Bgra::TRANSPARENT;
                }
                Bgra::from_bytes([px[o], px[o + 1], px[o + 2], px[o + 3]])
            }
        }
    }
}

impl Default for SpriteAtlas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SpriteAtlas;
    use alloc::vec;

    /// D5.4 done-when: filling many glyphs does not unbounded-grow without eviction.
    #[test]
    fn tiny_atlas_evicts() {
        let mut atlas = SpriteAtlas::with_max_tiles(8);
        for i in 0..64u32 {
            let _ = atlas.alloc_alpha(1, 1, vec![i as u8]);
        }
        assert_eq!(atlas.len(), 8);
    }

    #[test]
    fn intern_bgra_reuses_key() {
        let mut atlas = SpriteAtlas::new();
        let a = atlas.intern_bgra(7, 1, 1, vec![1, 2, 3, 255]);
        let b = atlas.intern_bgra(7, 1, 1, vec![9, 9, 9, 255]);
        assert_eq!(a.texture_id, b.texture_id);
        assert_eq!(atlas.len(), 1);
        let c = atlas.intern_bgra(0, 1, 1, vec![4, 5, 6, 255]);
        assert_ne!(a.texture_id, c.texture_id);
    }
}
