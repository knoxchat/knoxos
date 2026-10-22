// SPDX-License-Identifier: MIT
//! Single-threaded GPUI `App`: entities, windows, globals, dispatcher.
//!
//! No threads. The kernel UI task owns one `App` behind a `spin::Mutex`.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::any::{Any, TypeId};

use crate::entity::{Entity, EntityMap};
use crate::executor::PlatformDispatcher;
use crate::geometry::Bounds;
use crate::keymap::Keymap;
use crate::window::{Window, WindowId};

/// Application state for Knox GPUI.
pub struct App {
    entities: EntityMap,
    next_window: u64,
    windows: BTreeMap<u64, Window>,
    globals: Vec<(TypeId, Box<dyn Any + Send>)>,
    dispatcher: PlatformDispatcher,
    keymap: Keymap,
}

impl App {
    pub fn new() -> Self {
        Self {
            entities: EntityMap::new(),
            next_window: 1,
            windows: BTreeMap::new(),
            globals: Vec::new(),
            dispatcher: PlatformDispatcher::new(),
            keymap: Keymap::new(),
        }
    }

    pub fn entities(&self) -> &EntityMap {
        &self.entities
    }

    pub fn entities_mut(&mut self) -> &mut EntityMap {
        &mut self.entities
    }

    pub fn new_entity<T: Send + 'static>(&mut self, value: T) -> Entity<T> {
        self.entities.insert(value)
    }

    pub fn update<T: Send + 'static, R>(
        &mut self,
        entity: Entity<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        self.entities.update(entity, f)
    }

    pub fn read<T: Send + 'static, R>(&self, entity: Entity<T>, f: impl FnOnce(&T) -> R) -> R {
        self.entities.read(entity, f)
    }

    /// Open a window and return its handle (D2.2).
    pub fn open_window(&mut self, bounds: Bounds) -> WindowId {
        let id = WindowId::from_u64(self.next_window);
        self.next_window += 1;
        self.windows.insert(id.as_u64(), Window::new(id, bounds));
        id
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id.as_u64())
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id.as_u64())
    }

    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    pub fn windows_mut(&mut self) -> impl Iterator<Item = &mut Window> {
        self.windows.values_mut()
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn close_window(&mut self, id: WindowId) {
        self.windows.remove(&id.as_u64());
    }

    pub fn set_global<T: Send + 'static>(&mut self, value: T) {
        let tid = TypeId::of::<T>();
        self.globals.retain(|(t, _)| *t != tid);
        self.globals.push((tid, Box::new(value)));
    }

    pub fn global<T: Send + 'static>(&self) -> Option<&T> {
        let tid = TypeId::of::<T>();
        self.globals
            .iter()
            .find(|(t, _)| *t == tid)
            .and_then(|(_, v)| v.downcast_ref::<T>())
    }

    pub fn dispatcher(&self) -> &PlatformDispatcher {
        &self.dispatcher
    }

    pub fn dispatcher_mut(&mut self) -> &mut PlatformDispatcher {
        &mut self.dispatcher
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn keymap_mut(&mut self) -> &mut Keymap {
        &mut self.keymap
    }

    pub fn pump(&mut self) {
        self.dispatcher.pump();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<App>();
};

#[cfg(test)]
mod tests {
    use super::App;
    use crate::geometry::Bounds;

    /// D2.2 done-when: host test opens one window handle.
    #[test]
    fn opens_one_window_handle() {
        let mut app = App::new();
        let id = app.open_window(Bounds::from_xywh(10, 20, 32, 32));
        assert_eq!(app.window_count(), 1);
        let win = app.window(id).expect("window handle");
        assert_eq!(win.id(), id);
        assert_eq!(win.bounds(), Bounds::from_xywh(10, 20, 32, 32));
        assert!(win.is_dirty());
    }

    #[test]
    fn update_read_entity_and_global() {
        let mut app = App::new();
        let e = app.new_entity(1u32);
        app.update(e, |v| *v += 2);
        assert_eq!(app.read(e, |v| *v), 3);
        assert!(e.is_dirty(app.entities()));
        app.set_global(42u8);
        assert_eq!(app.global::<u8>().copied(), Some(42));
    }
}
