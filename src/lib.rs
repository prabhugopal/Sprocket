#![no_std]

#![feature(lang_items)]
#![feature(const_fn)]
#![feature(asm)]
#![feature(repr_simd)]
#![feature(alloc)]
#![feature(box_syntax)]
#![feature(drop_types_in_const)]
#![feature(global_allocator)]

#![allow(dead_code)]
#![cfg_attr(feature = "cargo-clippy", allow(empty_loop))]


extern crate rlibc;
#[macro_use]
extern crate alloc;
extern crate x86;
extern crate slice_cast;
extern crate smoltcp;
#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
extern crate spin;

extern crate pci;
extern crate simple_fs as fs;
extern crate mem_utils as mem;
extern crate kalloc;
extern crate spinlock;

#[macro_use]
mod console;
#[macro_use]
mod process;

mod vm;
mod traps;
mod mmu;
mod file;
mod picirq;
mod uart;
mod timer;
mod ide;
mod rtl8139;
mod logger;
mod service;

use mem::{PhysAddr, VirtAddr, Address};
pub use traps::trap;
use x86::shared::irq;
use service::Service;

#[global_allocator]
static ALLOCATOR: kalloc::RangeAlloc = kalloc::RANGE_ALLOC_INIT;

#[no_mangle]
pub extern "C" fn main() {
    println!("");
    for i in &[196, 202, 214, 34, 51, 57] {
        print!("[38;5;{}m{} ", i, "⚙");
    }
    println!("");

    logger::init().unwrap();
    info!("Initializing allocator");
    unsafe {
        let heap_start = VirtAddr::new(&mem::end as *const _ as usize + mem::PGSIZE).page_roundup();
        let heap_end = PhysAddr(4 * 1024 * 1024).to_virt();
        ALLOCATOR.init(heap_start, heap_end);
    }

    info!("Initializing kernel paging");
    vm::kvmalloc();
    info!("Initializing kernel segments");
    vm::seginit();
    info!("Configuring PIC");
    picirq::PIC.lock().picinit();
    info!("Setting up interrupt descriptor table");
    traps::trap_vector_init();
    timer::timerinit();
    info!("Loading new interrupt descriptor table");
    traps::idtinit();

    info!("Finishing allocator initialization");
    unsafe {
        ALLOCATOR.init(PhysAddr(4 * 1024 * 1024).to_virt(), mem::PHYSTOP.to_virt());
    }

    info!("Enumerating PCI");
    pci::enumerate();
    unsafe {
        *rtl8139::NIC.lock() = rtl8139::Rtl8139::init();
    }

    info!("COFFLOS initialization complete, jumping to user code");
    unsafe { irq::enable() };
    service::UserService::start();
    panic!("User application ended");
}

#[lang = "panic_fmt"]
#[no_mangle]
pub extern "C" fn panic_fmt(fmt: ::core::fmt::Arguments, file: &'static str, line: u32) -> ! {
    unsafe { irq::disable() };
    unsafe { console::CONSOLE.force_unlock() };
    spinlock::LOCK_COUNT.store(0, core::sync::atomic::Ordering::SeqCst);
    spinlock::INT_ENABLED.store(false, core::sync::atomic::Ordering::SeqCst);
    error!("Panic! An unrecoverable error occurred at {}:{}",
           file,
           line);
    error!("{}", fmt);
    logger::shutdown().unwrap();
    loop {}
}

#[lang = "eh_personality"]
#[no_mangle]
pub extern "C" fn eh_personality() {}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {}
}

const PTE_P: u32 = 0x001; // Present
const PTE_W: u32 = 0x002; // Writeable
const PTE_PS: u32 = 0x080; // Page Size

#[repr(C)]
pub struct EntryPgDir {
    align: [PageAligner4K; 0],
    array: [u32; 1024],
}

// NOTE!  This manually puts the entry in KERNBASE >> PDXSHIFT.  This is 512,
// but if you ever want to change those constants, CHANGE THIS TOO!
impl EntryPgDir {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    const fn new() -> EntryPgDir {
        EntryPgDir {
            align: [],
            array: [PTE_P | PTE_W | PTE_PS, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            PTE_P | PTE_W | PTE_PS, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    }
}

#[no_mangle]
pub static ENTRYPGDIR: EntryPgDir = EntryPgDir::new();

// NB This idiotic piece of code exists because Rust doesn't provide a way to ask
// that a variable be aligned on a certain boundary (the way that with GCC, you
// can use __align).  The workaround is to create a fictional SIMD type that
// must be aligned to 4K.  Then, you can put a zero-length array of type
// PageAligner4K at the start of an arbitrary struct, to force it to be aligned
// in a certain way.
//
// THIS IS INCREDIBLY FRAGILE AND MAY BREAK!!!
#[cfg_attr(rustfmt, rustfmt_skip)]
#[repr(simd)]
pub struct PageAligner4K(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64, u64, u64, u64,
                       u64, u64, u64, u64, u64, u64, u64);
