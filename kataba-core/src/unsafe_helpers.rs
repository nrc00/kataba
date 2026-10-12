//! Unsafe helpers (commented out).
//!
//! NOTE: The functions in this file are commented out because `kataba-core` adheres to a strict
//! `#![forbid(unsafe_code)]` policy (enforced in `Cargo.toml`). Keeping the core 100% safe Rust
//! ensures provable memory safety, simplifies fuzzing, and prevents undefined behavior.
//! These helpers are preserved here for reference only.

// /// # Safety
// ///
// /// The caller must guarantee that `ptr` is aligned, valid for reads, and
// /// points to a properly initialized `T`. The caller must also guarantee that
// /// no other reference to the same memory exists concurrently.
// pub unsafe fn read_unchecked<T>(ptr: *const T) -> T {
//     // SAFETY: precondition of this function.
//     unsafe { core::ptr::read(ptr) }
// }
// 
// /// # Safety
// ///
// /// The caller must guarantee that `ptr` is aligned, valid for writes, and
// /// points to memory of size at least `size_of::<T>()`. The caller must also
// /// guarantee that no other reference to the same memory exists concurrently.
// pub unsafe fn write_unchecked<T>(ptr: *mut T, value: T) {
//     // SAFETY: precondition of this function.
//     unsafe { core::ptr::write(ptr, value) }
// }