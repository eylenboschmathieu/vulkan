#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Allocation { pub offset: usize, pub size: usize }
type FreeBlock = Allocation;

fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

#[derive(Debug)]
pub struct Allocator {
    free_list: Vec<Allocation>,
    alignment: usize,
}

impl Allocator {
    /// Create a freelist allocator of a given size
    pub fn new(size: usize, alignment: usize) -> Self {
        Self {
            free_list: vec![FreeBlock { offset: 0, size }],
            alignment,
        }
    }

    /// Returns an option of type Allocation if allocation succeeded, None if not
    pub fn alloc(&mut self, size: usize) -> Option<Allocation> {
        for i in 0..self.free_list.len() {
            let block = self.free_list[i];

            let aligned_start = align_up(block.offset, self.alignment);
            let padding = aligned_start - block.offset;

            if block.size < padding + size {
                continue;
            }

            let alloc = Allocation {
                offset: aligned_start,
                size,
            };

            let remaining = block.size - (padding + size);
            let mut new_blocks = Vec::new();

            if padding > 0 {
                new_blocks.push(FreeBlock {
                    offset: block.offset,
                    size: padding,
                });
            }

            if remaining > 0 {
                new_blocks.push(FreeBlock {
                    offset: aligned_start + size,
                    size: remaining,
                });
            }

            self.free_list.remove(i);
            self.free_list.splice(i..i, new_blocks);

            return Some(alloc)
        }
        None
    }

    /// Free an Allocation
    pub fn free(&mut self, allocation: Allocation) {
        let pos = self.free_list
            .iter()
            .position(|alloc| alloc.offset > allocation.offset)
            .unwrap_or(self.free_list.len());

        self.free_list.insert(pos, allocation as FreeBlock);
        self.coalesce();
    }

    fn coalesce(&mut self) {
        if self.free_list.is_empty() {
            return;
        }

        let mut merged = Vec::with_capacity(self.free_list.len());
        let mut current = self.free_list[0];

        for &next in &self.free_list[1..] {
            if current.offset + current.size == next.offset {
                current.size += next.size;  // Merge
            } else {
                merged.push(current);
                current = next;
            }
        }

        merged.push(current);
        self.free_list = merged;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_freelist() {
        let mut a = Allocator::new(64, 4);
        let alloc1 = a.alloc(16).unwrap();
        let alloc2 = a.alloc(8).unwrap();
        let alloc3 = a.alloc(16).unwrap();
        let alloc4 = a.alloc(4).unwrap();

        assert_eq!(alloc1, Allocation { offset: 0, size: 16 });
        assert_eq!(alloc2, Allocation { offset: 16, size: 8 });
        assert_eq!(alloc3, Allocation { offset: 24, size: 16 });
        assert_eq!(alloc4, Allocation { offset: 40, size: 4 });

        a.free(alloc2);
        let alloc5 = a.alloc(4).unwrap();
        let alloc6 = a.alloc(16).unwrap();

        assert_eq!(alloc5, Allocation { offset: 16, size: 4 });
        assert_eq!(alloc6, Allocation { offset: 44, size: 16 });

        let overallocate = a.alloc(50);
        assert_eq!(overallocate, None);
    }
}