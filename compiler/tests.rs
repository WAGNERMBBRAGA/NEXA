// tests.rs - Example usage and validation of BumpHeap implementation

use bump::BumpHeap;

fn main() {
    println!("=== Bump Heap Allocation Test ===\n");
    
    // Create a new bump allocator with 1024 bytes capacity
    let mut heap = BumpHeap::new(1024);
    
    println!("Initial stats: {:?}", heap.stats());
    
    // Allocate some values
    if let Some(ptr1) = heap.alloc(64) {
        println!("Allocated 64 bytes at {:p}", ptr1);
        unsafe {
            *ptr1 as *const u8 = 0x42;
        }
    }
    
    if let Some(ptr2) = heap.alloc(128) {
        println!("Allocated 128 bytes at {:p}", ptr2);
    }
    
    // Check available space
    if let Some(ptr3) = heap.alloc(256) {
        println!("Allocated 256 bytes at {:p}", ptr3);
    } else {
        println!("Failed to allocate 256 bytes - out of space!");
    }
    
    // Get stats after allocations
    let stats = heap.stats();
    println!("\nAfter allocations: {:?}", stats);
    
    // Test alignment allocation
    if let Some(ptr) = heap.alloc_align(32, 8) {
        println!("Aligned allocation (8-byte): {:p}", ptr);
    } else {
        println!("Failed to allocate with 8-byte alignment");
    }
    
    // Check can_alloc before attempting
    println!("\nCan allocate 512 bytes? {}", heap.can_alloc(512));
    
    // Get allocation count
    println!("Total allocations: {}", heap.alloc_count());
}

#[cfg(test)]
mod bump_heap_tests {
    use super::BumpHeap;
    
    #[test]
    fn test_basic_allocation() {
        let mut heap = BumpHeap::new(100);
        
        assert!(heap.can_alloc(50));
        assert_eq!(heap.alloc_count(), 0);
        
        if let Some(ptr) = heap.alloc(50) {
            unsafe { *ptr as *const u8 = 42 }
            assert_eq!(heap.alloc_count(), 1);
        } else {
            panic!("Should be able to allocate");
        }
    }
    
    #[test]
    fn test_out_of_space() {
        let mut heap = BumpHeap::new(50);
        
        // Fill the buffer completely
        for i in 1..20 {
            assert!(heap.alloc(i).is_some());
        }
        
        // Should fail to allocate more after filling
        assert!(!heap.can_alloc(1));
    }
    
    #[test]
    fn test_alignment() {
        let mut heap = BumpHeap::new(64);
        
        if let Some(ptr) = heap.alloc_align(8, 8) {
            // Verify alignment
            assert_eq!(ptr as usize % 8, 0);
        } else {
            panic!("Alignment allocation failed");
        }
    }
    
    #[test]
    fn test_reset() {
        let mut heap = BumpHeap::new(100);
        
        // Make some allocations
        for _ in 0..10 {
            assert!(heap.alloc(5).is_some());
        }
        
        assert_eq!(heap.stats().allocated, 50);
        
        // Reset the allocator
        heap.reset();
        
        assert_eq!(heap.stats().allocated, 0);
    }
}
