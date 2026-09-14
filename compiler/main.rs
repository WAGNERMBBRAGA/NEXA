// Example binary for testing BumpHeap implementation

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
