// SPDX-License-Identifier: MIT
//! Slotmap entity handles for Knox GPUI.
//!
//! Type names follow Zed GPUI (`EntityId`, `Entity`) as a design reference
//! (Apache-2.0). This file is new `no_std` code: App-owned slots, no
//! `parking_lot`, no `std::any` thread locals. `Entity<T>` is a `Copy` id;
//! values live in [`EntityMap`] so the map stays `Send` for the kernel UI task.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;
use core::marker::PhantomData;

/// Slotmap key: index + generation. Stale handles miss after reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub const fn as_u64(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u64())
    }
}

/// Typed handle. Access goes through [`EntityMap`] / [`crate::App`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Entity<T> {
    id: EntityId,
    _pd: PhantomData<fn() -> T>,
}

impl<T> Entity<T> {
    pub const fn id(self) -> EntityId {
        self.id
    }

    /// Mark this entity dirty (needs a paint).
    pub fn notify(self, map: &mut EntityMap) {
        map.set_dirty(self.id, true);
    }

    pub fn is_dirty(self, map: &EntityMap) -> bool {
        map.is_dirty(self.id)
    }

    pub fn mark_clean(self, map: &mut EntityMap) {
        map.set_dirty(self.id, false);
    }
}

struct Slot {
    generation: u32,
    value: Option<Box<dyn Any + Send>>,
    dirty: bool,
}

/// Generation-counted slotmap. Values are `T: Send + 'static`.
pub struct EntityMap {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl EntityMap {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert<T: Send + 'static>(&mut self, value: T) -> Entity<T> {
        let boxed: Box<dyn Any + Send> = Box::new(value);
        let index = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(boxed);
            slot.dirty = false;
            index
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 0,
                value: Some(boxed),
                dirty: false,
            });
            index
        };
        let generation = self.slots[index as usize].generation;
        Entity {
            id: EntityId { index, generation },
            _pd: PhantomData,
        }
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.slot(id).is_some()
    }

    pub fn remove(&mut self, id: EntityId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return false;
        };
        if slot.generation != id.generation || slot.value.is_none() {
            return false;
        }
        slot.value = None;
        slot.dirty = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        true
    }

    pub fn set_dirty(&mut self, id: EntityId, dirty: bool) {
        if let Some(slot) = self.slot_mut(id) {
            slot.dirty = dirty;
        }
    }

    pub fn is_dirty(&self, id: EntityId) -> bool {
        self.slot(id).map(|s| s.dirty).unwrap_or(false)
    }

    pub fn update<T: Send + 'static, R>(
        &mut self,
        entity: Entity<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        let slot = self
            .slot_mut(entity.id)
            .expect("knox-gpui: stale entity in update");
        slot.dirty = true;
        let value = slot
            .value
            .as_mut()
            .and_then(|v| v.downcast_mut::<T>())
            .expect("knox-gpui: entity type mismatch");
        f(value)
    }

    pub fn read<T: Send + 'static, R>(&self, entity: Entity<T>, f: impl FnOnce(&T) -> R) -> R {
        let slot = self
            .slot(entity.id)
            .expect("knox-gpui: stale entity in read");
        let value = slot
            .value
            .as_ref()
            .and_then(|v| v.downcast_ref::<T>())
            .expect("knox-gpui: entity type mismatch");
        f(value)
    }

    fn slot(&self, id: EntityId) -> Option<&Slot> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation == id.generation && slot.value.is_some() {
            Some(slot)
        } else {
            None
        }
    }

    fn slot_mut(&mut self, id: EntityId) -> Option<&mut Slot> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation == id.generation && slot.value.is_some() {
            Some(slot)
        } else {
            None
        }
    }
}

impl Default for EntityMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{EntityId, EntityMap};

    /// D2.1 done-when: host test notify marks dirty.
    #[test]
    fn notify_marks_dirty() {
        let mut map = EntityMap::new();
        let e = map.insert(0u32);
        assert!(!e.is_dirty(&map));
        e.notify(&mut map);
        assert!(e.is_dirty(&map));
        e.mark_clean(&mut map);
        assert!(!e.is_dirty(&map));
        map.update(e, |v| *v = 7);
        assert!(e.is_dirty(&map));
        assert_eq!(map.read(e, |v| *v), 7);
    }

    #[test]
    fn slotmap_reuses_index_new_generation() {
        let mut map = EntityMap::new();
        let a = map.insert(1u32);
        let id = a.id();
        assert!(map.remove(id));
        assert!(!map.contains(id));
        let b = map.insert(2u32);
        assert_eq!(b.id().index(), id.index());
        assert_ne!(b.id().generation(), id.generation());
        assert!(!map.contains(id));
        assert!(map.contains(b.id()));
        assert_eq!(map.read(b, |v| *v), 2);
    }

    #[test]
    fn entity_id_packs() {
        let id = EntityId {
            index: 3,
            generation: 1,
        };
        assert_eq!(id.as_u64(), (1u64 << 32) | 3);
        assert_eq!(alloc::format!("{id}"), alloc::format!("{}", id.as_u64()));
    }
}
