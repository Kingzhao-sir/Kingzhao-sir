//! Zero-Allocation Memory Arena
//! Uses bump allocation for hot path to eliminate malloc/free overhead

use std::cell::RefCell;

pub struct Arena {
    buffer: Vec<u8>,
    offset: RefCell<usize>,
}

impl Arena {
    pub fn new(size_mb: usize) -> Self {
        let size_bytes = size_mb * 1024 * 1024;
        Self {
            buffer: vec![0u8; size_bytes],
            offset: RefCell::new(0),
        }
    }
    
    #[inline]
    pub fn reset(&self) {
        *self.offset.borrow_mut() = 0;
    }
    
    #[inline]
    pub fn alloc<T: Copy>(&self, value: T) -> &mut T {
        let mut offset = self.offset.borrow_mut();
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        
        // Align offset
        let aligned_offset = (*offset + align - 1) & !(align - 1);
        
        if aligned_offset + size > self.buffer.len() {
            panic!("Arena exhausted");
        }
        
        let ptr = unsafe {
            let ptr = self.buffer.as_mut_ptr().add(aligned_offset) as *mut T;
            ptr.write(value);
            &mut *ptr
        };
        
        *offset = aligned_offset + size;
        ptr
    }
    
    #[inline]
    pub fn available_bytes(&self) -> usize {
        self.buffer.len() - *self.offset.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_arena_allocation() {
        let arena = Arena::new(1);
        let val = arena.alloc(42i64);
        assert_eq!(*val, 42);
    }
    
    #[test]
    fn test_arena_reset() {
        let arena = Arena::new(1);
        let _val1 = arena.alloc(100i64);
        let before_reset = arena.available_bytes();
        arena.reset();
        let after_reset = arena.available_bytes();
        assert!(after_reset > before_reset);
    }
}
