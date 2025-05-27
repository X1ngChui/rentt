use std::ptr;

#[inline]
pub(crate) unsafe fn swap_remove_unchecked<T>(vec: &mut Vec<T>, index: usize) -> T {
    let len = vec.len();
    debug_assert!(index < len);
    unsafe {
        // We replace self[index] with the last element. Note that if the
        // bounds check above succeeds there must be a last element (which
        // can be self[index] itself).
        let value = ptr::read(vec.as_ptr().add(index));
        let base_ptr = vec.as_mut_ptr();
        ptr::copy(base_ptr.add(len - 1), base_ptr.add(index), 1);
        vec.set_len(len - 1);
        value
    }
}
