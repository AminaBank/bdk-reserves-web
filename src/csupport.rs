//! C runtime symbols that `bitcoinconsensus` needs on wasm.
//!
//! `bitcoinconsensus` compiles Bitcoin Core's script interpreter, so the build
//! needs a C++ toolchain and standard library for wasm. build.sh supplies those
//! from the wasi-sdk, but deliberately does *not* link wasi-libc: that would put
//! a second allocator in the module alongside Rust's and add a WASI import
//! surface the browser would have to fill in.
//!
//! Instead libc++ is backed by the handful of symbols below. `malloc`/`free`
//! forward to Rust's allocator so there is exactly one heap, and the rest are
//! either real implementations or inert stubs on paths this module never
//! reaches. The result imports nothing but the wasm-bindgen glue.

use core::ffi::{c_char, c_int, c_void};
use std::alloc::{alloc, dealloc, Layout};

/// Size header stored in front of every allocation. 16 bytes both records the
/// layout `free` has to reconstruct and keeps the returned pointer aligned to
/// wasm32's `max_align_t`.
const HDR: usize = 16;

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let total = match size.checked_add(HDR) {
        Some(total) => total,
        None => return core::ptr::null_mut(),
    };
    let layout = match Layout::from_size_align(total, HDR) {
        Ok(layout) => layout,
        Err(_) => return core::ptr::null_mut(),
    };
    let base = alloc(layout);
    if base.is_null() {
        return core::ptr::null_mut();
    }
    (base as *mut usize).write(total);
    base.add(HDR) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    let bytes = match count.checked_mul(size) {
        Some(bytes) => bytes,
        None => return core::ptr::null_mut(),
    };
    let ptr = malloc(bytes);
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr as *mut u8, 0, bytes);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(ptr);
        return core::ptr::null_mut();
    }
    let base = (ptr as *mut u8).sub(HDR);
    let old_total = (base as *mut usize).read();
    let new = malloc(size);
    if new.is_null() {
        return core::ptr::null_mut();
    }
    let keep = core::cmp::min(old_total - HDR, size);
    core::ptr::copy_nonoverlapping(ptr as *const u8, new as *mut u8, keep);
    free(ptr);
    new
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let base = (ptr as *mut u8).sub(HDR);
    let total = (base as *mut usize).read();
    dealloc(base, Layout::from_size_align_unchecked(total, HDR));
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    let (mut a, mut b) = (a as *const u8, b as *const u8);
    loop {
        let (x, y) = (*a, *b);
        if x != y {
            return x as c_int - y as c_int;
        }
        if x == 0 {
            return 0;
        }
        a = a.add(1);
        b = b.add(1);
    }
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    core::arch::wasm32::unreachable()
}

#[no_mangle]
pub extern "C" fn __assert_fail(
    _expr: *const c_char,
    _file: *const c_char,
    _line: c_int,
    _func: *const c_char,
) -> ! {
    core::arch::wasm32::unreachable()
}

/// Static destructors get registered but never run: the module lives as long as
/// the page does, so there is nothing to tear down.
#[no_mangle]
pub extern "C" fn __cxa_atexit(_f: *mut c_void, _arg: *mut c_void, _dso: *mut c_void) -> c_int {
    0
}

/// The wasi-sdk ships libc++ and libc++abi built without exception support, so
/// the Itanium throw path is unresolved. bitcoinconsensus only reaches it
/// through tinyformat's `TINYFORMAT_ERROR`, which fires on malformed format
/// strings inside Bitcoin Core itself rather than on anything callers pass in,
/// so trapping is the honest behaviour. The slot exists because the compiler
/// constructs the exception object before handing it to `__cxa_throw`.
///
/// `UnsafeCell` rather than `static mut`, so handing out the pointer needs no
/// unsafe block on any compiler this builds with. wasm is single threaded, so
/// the `Sync` claim costs nothing.
struct ExceptionSlot(core::cell::UnsafeCell<[u8; 256]>);
unsafe impl Sync for ExceptionSlot {}
static EXCEPTION_SLOT: ExceptionSlot = ExceptionSlot(core::cell::UnsafeCell::new([0; 256]));

#[no_mangle]
pub extern "C" fn __cxa_allocate_exception(_size: usize) -> *mut c_void {
    EXCEPTION_SLOT.0.get() as *mut c_void
}

#[no_mangle]
pub extern "C" fn __cxa_free_exception(_ptr: *mut c_void) {}

#[no_mangle]
pub extern "C" fn __cxa_throw(_ex: *mut c_void, _ty: *mut c_void, _dt: *mut c_void) -> ! {
    core::arch::wasm32::unreachable()
}

// libc++ pulls in stream error reporting that nothing here can reach, since the
// module never writes to a stream. Keep the symbols inert rather than
// implementing stdio.
#[no_mangle]
pub static mut stderr: *mut c_void = core::ptr::null_mut();

#[no_mangle]
pub extern "C" fn fprintf(_stream: *mut c_void, _fmt: *const c_char, _args: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn vfprintf(_stream: *mut c_void, _fmt: *const c_char, _args: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn fputc(_ch: c_int, _stream: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub extern "C" fn fwrite(
    _buf: *const c_void,
    _size: usize,
    _count: usize,
    _stream: *mut c_void,
) -> usize {
    0
}

static ERROR_STRING: [u8; 8] = *b"unknown\0";

#[no_mangle]
pub extern "C" fn strerror(_errnum: c_int) -> *mut c_char {
    ERROR_STRING.as_ptr() as *mut c_char
}
