# Bump Allocation Pattern Implementation

This module implements a bump allocator pattern in Rust, providing efficient memory allocation without actual deallocation.

## Core Components

### BumpHeap
A simple bump allocator that:
- Allocates memory from a contiguous block
- Tracks allocation statistics
- Supports aligned allocations
- Provides reset functionality (no actual memory deallocation)

### StringBumpHeap
Optimized for string allocation, avoiding heap fragmentation by reserving capacity upfront.

### AllocPool
Pre-allocated pool of BumpHeaps for high-performance scenarios with automatic rotation through available heaps.

## Key Features

- **No deallocation**: Memory is never freed, only reused (fast allocation!)
- **Statistics tracking**: Monitor available space and allocation count
- **Alignment support**: Allocate values with custom alignment requirements
- **Pool management**: Multiple heaps for large-scale applications
- **Pre-allocation**: StringBumpHeap reserves capacity before writing data

## Usage Example

```rust
use bump::BumpHeap;

let mut heap = BumpHeap::new(1024); // 1KB buffer

// Allocate some memory
if let Some(ptr) = heap.alloc(64) {
    unsafe { *ptr as *const u8 = 42 }
}

// Check statistics
println!("{:?}", heap.stats());

// Reset (no actual deallocation)
heap.reset();
```

## Performance Benefits

- O(1) allocation time (just update a pointer)
- No memory fragmentation
- Predictable performance
- Ideal for fixed-size buffers and game engine data structures
