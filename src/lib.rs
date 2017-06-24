#![no_std]

#![feature(lang_items)]
#![feature(const_fn)]
#![feature(asm)]
#![feature(repr_simd)]
#![feature(alloc)]
#![feature(box_syntax)]
#![feature(drop_types_in_const)]

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

extern crate pci;
extern crate simple_fs as fs;
extern crate mem_utils as mem;
extern crate kalloc;

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
#[macro_use]
mod console;
#[macro_use]
mod process;



mod flags;
mod vm;
mod traps;
mod mmu;
//mod file;
mod picirq;
mod uart;
mod timer;
//mod sleeplock;
mod ide;
mod rtl8139;
mod logger;

use mem::{PhysAddr, Address};
pub use traps::trap;
use x86::shared::irq;
use alloc::borrow::ToOwned;

#[no_mangle]
pub extern "C" fn main() {
    unsafe {
        console::CONSOLE2 = Some(console::Console::new());
    }
    println!("COFFLOS OK!");
    println!("Initializing allocator");
    unsafe {
        kalloc::kinit1(&mut kalloc::end,
                       PhysAddr(4 * 1024 * 1024).to_virt().addr() as *mut u8);
    }
    logger::init().unwrap();


    println!("Initializing kernel paging");
    vm::kvmalloc();
    println!("Initializing kernel segments");
    vm::seginit();
    println!("Configuring PIC");
    picirq::picinit();
    println!("Setting up interrupt descriptor table");
    traps::trap_vector_init();
    timer::timerinit();
    println!("Loading new interrupt descriptor table");
    traps::idtinit();



    println!("Finishing allocator initialization");
    unsafe {
        kalloc::kinit2(PhysAddr(4 * 1024 * 1024).to_virt().addr() as *mut u8,
                       mem::PHYSTOP.to_virt().addr() as *mut u8);
    }

    //unsafe { kalloc::validate() };


    println!("Reading root fs");

    let mut fs = fs::FileSystem { disk: ide::Ide::init() };

    let inum = fs.namex(b"/", b"README").unwrap();
    let inode = fs.read_inode(fs::ROOT_DEV, inum);
    match inode {
        Ok(i) => {
            println!("OK! Found 'README' at {}", inum);
            println!("Size: {}", i.size);
            println!("======================================================================");

            let mut buf = [0; fs::BLOCKSIZE];
            let mut off = 0;
            while let Ok(n) = fs.read(&i, &mut buf, off) {
                let s = ::core::str::from_utf8(&buf[..n]);
                match s {
                    Ok(s) => print!("{}", s),
                    Err(e) => {
                        println!("error, up to {}", e.valid_up_to());
                        println!("at offset{}. Char is '{:x}'", off, buf[e.valid_up_to()]);
                    }
                }
                off += fs::BLOCKSIZE as u32;
            }
            println!("======================================================================");
        }
        Err(_) => println!("Something broke :("),
    }
    println!("Enumerating PCI");
    pci::enumerate();
    unsafe {
        rtl8139::NIC = rtl8139::Rtl8139::init();
    }

    use alloc::string::String;

    let inum = fs.namex(b"/", b"small.html").unwrap();
    let inode = fs.read_inode(fs::ROOT_DEV, inum);
    let html = match inode {
        Ok(i) => {
            let mut buf = vec![0; i.size as usize];
            fs.read(&i, &mut buf, 0).unwrap();
            String::from_utf8(buf).unwrap().replace("${{VERSION}}", env!("CARGO_PKG_VERSION"))
        }
        Err(_) => panic!("Couldn't load HTML file"),
    };

    let header: String = "HTTP/1.1 200 OK\r\n\r\n".to_owned();
    let http = header + html.as_str();

    unsafe { irq::enable() };
    loop {
        use smoltcp::iface::{EthernetInterface, SliceArpCache, ArpCache};
        use smoltcp::wire::{EthernetAddress, IpAddress};
        use smoltcp::socket::{AsSocket, SocketSet};
        use smoltcp::socket::{TcpSocket, TcpSocketBuffer};
        use smoltcp::Error;
        use alloc::boxed::Box;
        use core::str;

        let arp_cache = SliceArpCache::new(vec![Default::default(); 8]);
        let hw_addr = unsafe { EthernetAddress(rtl8139::NIC.as_mut().unwrap().mac_address()) };

        let protocol_addr = IpAddress::v4(10, 0, 0, 4);
        let nic = unsafe { rtl8139::NIC.as_mut().unwrap() };
        let mut iface = EthernetInterface::new(nic,
                                               Box::new(arp_cache) as Box<ArpCache>,
                                               hw_addr,
                                               [protocol_addr]);

        let tcp_rx_buffer = TcpSocketBuffer::new(vec![0; 4096]);
        let tcp_tx_buffer = TcpSocketBuffer::new(vec![0; 4096]);
        let tcp_socket = TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer);

        let mut sockets = SocketSet::new(vec![]);
        let tcp_handle = sockets.add(tcp_socket);

        loop {
            {
                let socket: &mut TcpSocket = sockets.get_mut(tcp_handle).as_socket();
                if !socket.is_open() {
                    socket.listen(80).unwrap();
                }

                if socket.can_recv() {
                    let _ = socket.recv(200);
                    if socket.can_send() {
                        let seconds = unsafe { timer::TICKS } / 100;
                        const SECONDS_PER_MINUTE: u32 = 60;
                        const MINUTES_PER_HOUR: u32 = 60;
                        const HOURS_PER_DAY: u32 = 24;
                        const SECONDS_PER_DAY: u32 = (SECONDS_PER_MINUTE * MINUTES_PER_HOUR *
                                                      HOURS_PER_DAY);
                        const SECONDS_PER_HOUR: u32 = (SECONDS_PER_MINUTE * MINUTES_PER_HOUR);

                        let days = seconds / SECONDS_PER_DAY;
                        let hours = (seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR;
                        let minutes = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
                        let seconds = seconds % SECONDS_PER_MINUTE;

                        let time =
                            format!("{} days, {}:{:02}:{:02}", days, hours, minutes, seconds);

                        socket.send_slice(http.replace("${{TIME}}", &time).as_str().as_bytes())
                            .unwrap();
                        println!("socket closing");
                        socket.close();
                    }
                }
            }

            match iface.poll(&mut sockets, 10) {
                Ok(()) | Err(Error::Exhausted) => (),
                Err(e) => println!("poll error: {}", e),
            }
        }

    }
}


#[lang = "panic_fmt"]
#[no_mangle]
pub extern "C" fn panic_fmt(fmt: ::core::fmt::Arguments, file: &'static str, line: u32) -> ! {
    println!("Panic! An unrecoverable error occurred at {}:{}",
             file,
             line);
    println!("{}", fmt);
    unsafe {
        irq::disable();
        x86::shared::halt();
    }
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
pub static mut ENTRYPGDIR: EntryPgDir = EntryPgDir::new();

// This idiotic piece of code exists because Rust doesn't provide a way to ask
// that a variable be aligned on a certain boundary (the way that with GCC, you
// can use __align).  The workaround is to create a fictional SIMD type that must be aligned to 4K.  Then, you can put a zero-length array of type PageAligner4K at the start of an arbitrary struct, to force it to be aligned in a certain way.
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
