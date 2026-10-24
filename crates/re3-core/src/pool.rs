//! Fixed-size object pools with generation-checked handles (`templates.h`: `CPool`, `CStore`).
//!
//! A handle is `(slot << 8) | flag`, where the low byte holds the slot's 7-bit generation id.
//! Scripts, saves and the game logic pass these handles around instead of pointers; an old
//! handle stops resolving once its slot is reused (unless the id counter wraps).

const ID_MASK: u8 = 0x7f;
const FREE: u8 = 0x80;

#[derive(Debug, Clone)]
pub struct Pool<T> {
    entries: Vec<Option<T>>,
    flags: Vec<u8>,
    alloc_ptr: i32,
}

impl<T> Pool<T> {
    pub fn new(size: usize) -> Self {
        Self { entries: (0..size).map(|_| None).collect(), flags: vec![FREE; size], alloc_ptr: 0 }
    }

    pub fn size(&self) -> usize {
        self.flags.len()
    }

    fn is_free(&self, i: usize) -> bool {
        self.flags[i] & FREE != 0
    }

    fn id(&self, i: usize) -> u8 {
        self.flags[i] & ID_MASK
    }

    /// `New()`: takes the next free slot after the allocation cursor, wrapping once.
    /// Returns the new entry's handle, or `None` if the pool is full.
    pub fn insert(&mut self, value: T) -> Option<i32> {
        let size = self.size() as i32;
        let mut wrapped = false;
        loop {
            self.alloc_ptr += 1;
            if self.alloc_ptr >= size {
                self.alloc_ptr = 0;
                if wrapped {
                    return None;
                }
                wrapped = true;
            }
            if self.is_free(self.alloc_ptr as usize) {
                break;
            }
        }
        let i = self.alloc_ptr as usize;
        self.flags[i] = (self.id(i).wrapping_add(1)) & ID_MASK; // not free, id + 1
        self.entries[i] = Some(value);
        Some(self.handle_of(i))
    }

    /// `New(handle)`: claim exactly the slot and id a saved game recorded.
    pub fn insert_at_handle(&mut self, handle: i32, value: T) -> Option<()> {
        let i = usize::try_from(handle >> 8).ok().filter(|&i| i < self.size())?;
        self.flags[i] = (handle & ID_MASK as i32) as u8;
        self.entries[i] = Some(value);
        // the cursor goes back to the first free slot
        self.alloc_ptr = (0..self.size()).find(|&k| self.is_free(k)).unwrap_or(self.size()) as i32;
        Some(())
    }

    /// `Delete()`.
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.size() || self.is_free(index) {
            return None;
        }
        self.flags[index] |= FREE;
        if (index as i32) < self.alloc_ptr {
            self.alloc_ptr = index as i32;
        }
        self.entries[index].take()
    }

    pub fn remove_handle(&mut self, handle: i32) -> Option<T> {
        let i = self.index_of(handle)?;
        self.remove(i)
    }

    /// Slot of a handle if it is still current (`GetAt` without the reference).
    pub fn index_of(&self, handle: i32) -> Option<usize> {
        if handle < 0 {
            return None;
        }
        let i = (handle >> 8) as usize;
        (i < self.size() && self.flags[i] == (handle & 0xFF) as u8).then_some(i)
    }

    /// Lends the entry out without freeing its slot; give it back with [`Pool::restore`].
    pub fn take(&mut self, handle: i32) -> Option<T> {
        let i = self.index_of(handle)?;
        self.entries[i].take()
    }

    pub fn restore(&mut self, handle: i32, value: T) {
        if let Some(i) = self.index_of(handle) {
            self.entries[i] = Some(value);
        }
    }

    /// `GetAt(handle)`.
    pub fn get(&self, handle: i32) -> Option<&T> {
        self.entries[self.index_of(handle)?].as_ref()
    }

    pub fn get_mut(&mut self, handle: i32) -> Option<&mut T> {
        let i = self.index_of(handle)?;
        self.entries[i].as_mut()
    }

    /// `GetSlot(i)`: the entry in slot `i` if occupied.
    pub fn slot(&self, i: usize) -> Option<&T> {
        self.entries.get(i)?.as_ref()
    }

    pub fn slot_mut(&mut self, i: usize) -> Option<&mut T> {
        self.entries.get_mut(i)?.as_mut()
    }

    /// `GetIndex(entry)`: the handle for the entry in slot `i`.
    pub fn handle_of(&self, i: usize) -> i32 {
        self.flags[i] as i32 + ((i as i32) << 8)
    }

    /// `GetNoOfUsedSpaces()`.
    pub fn used(&self) -> usize {
        self.flags.iter().filter(|f| *f & FREE == 0).count()
    }

    /// Occupied slots as `(slot, entry)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.entries.iter().enumerate().filter_map(|(i, e)| e.as_ref().map(|e| (i, e)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
        self.entries.iter_mut().enumerate().filter_map(|(i, e)| e.as_mut().map(|e| (i, e)))
    }

    /// Empties the pool but keeps the generation ids (`CWorld` shutdown deletes entries one by one).
    pub fn clear(&mut self) {
        for i in 0..self.size() {
            self.remove(i);
        }
    }
}

/// `CStore`: append-only table that is cleared as a whole.
#[derive(Debug, Clone)]
pub struct Store<T> {
    items: Vec<T>,
    capacity: usize,
}

impl<T> Store<T> {
    pub fn new(capacity: usize) -> Self {
        Self { items: Vec::new(), capacity }
    }

    /// `Alloc()`: the original asserts when the table is full.
    pub fn alloc(&mut self, value: T) -> Result<usize, &'static str> {
        if self.items.len() >= self.capacity {
            return Err("store is full (the original size needs increasing)");
        }
        self.items.push(value);
        Ok(self.items.len() - 1)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.items.get(i)
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.items.get_mut(i)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_encode_slot_and_generation() {
        let mut p = Pool::new(4);
        let a = p.insert("a").unwrap();
        // the cursor starts at 0 and advances before use: slot 1 is taken first, generation 1
        assert_eq!(a, (1 << 8) | 1);
        let b = p.insert("b").unwrap();
        assert_eq!(b >> 8, 2);
        assert_eq!((p.get(a), p.get(b)), (Some(&"a"), Some(&"b")));
        assert_eq!(p.used(), 2);
        assert_eq!(p.handle_of(1), a);
    }

    #[test]
    fn stale_handles_stop_resolving() {
        let mut p = Pool::new(3);
        let a = p.insert(1).unwrap();
        assert_eq!(p.remove_handle(a), Some(1));
        assert_eq!(p.get(a), None);
        // fill the pool; slot 1 comes round again with the next generation
        let mut handles = vec![];
        while let Some(h) = p.insert(2) {
            handles.push(h);
        }
        // slots 2, 0 (after the wrap), then the freed slot 1 again with the next generation
        assert_eq!(handles.iter().map(|h| h >> 8).collect::<Vec<_>>(), [2, 0, 1]);
        assert_eq!(handles[2] & 0xFF, 2);
        assert_eq!(p.get(a), None, "old handle must not see the new occupant");
        assert_eq!(p.get(-1), None);
        assert_eq!(p.get(99 << 8), None);
        assert!(p.insert(3).is_none(), "full");
    }

    #[test]
    fn delete_moves_the_cursor_back() {
        let mut p = Pool::new(5);
        let hs: Vec<i32> = (0..4).map(|i| p.insert(i).unwrap()).collect();
        p.remove_handle(hs[0]);
        // the cursor sits on the freed slot 1 but advances before it allocates, so it runs on
        // past the full tail and wraps to slot 0 first; the freed slot comes next
        assert_eq!(p.insert(9).unwrap() >> 8, 0);
        assert_eq!(p.insert(8).unwrap() >> 8, 1);
        assert_eq!(p.iter().count(), 5);
    }

    #[test]
    fn generation_wraps_at_seven_bits() {
        let mut p = Pool::new(2);
        let mut last = 0;
        for _ in 0..130 {
            last = p.insert(()).unwrap();
            p.remove_handle(last);
        }
        assert!(last & 0xFF <= 0x7f);
        let h = p.insert(()).unwrap();
        assert!(h & 0xFF <= 0x7f && h & 0xFF != 0x80);
    }

    #[test]
    fn restoring_a_saved_handle() {
        let mut p = Pool::new(4);
        p.insert_at_handle((2 << 8) | 5, "saved").unwrap();
        assert_eq!(p.get((2 << 8) | 5), Some(&"saved"));
        assert_eq!(p.get((2 << 8) | 4), None);
        assert!(p.insert_at_handle(99 << 8, "x").is_none());
        // the next insert starts from the first free slot
        let h = p.insert("new").unwrap();
        assert_eq!(h >> 8, 1);
    }

    #[test]
    fn store_has_a_hard_limit() {
        let mut s = Store::new(2);
        assert_eq!(s.alloc('a'), Ok(0));
        assert_eq!(s.alloc('b'), Ok(1));
        assert!(s.alloc('c').is_err());
        s.clear();
        assert!(s.is_empty() && s.alloc('d').is_ok());
    }
}
