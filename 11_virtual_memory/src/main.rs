// SPDX-License-Identifier: MIT
//
// Copyright (c) 2018-2019 Andre Richter <andre.o.richter@gmail.com>

// Rust embedded logo for `make doc`.
#![doc(html_logo_url = "https://git.io/JeGIp")]

//! The `kernel`
//!
//! The `kernel` is composed by glueing together code from
//!
//!   - [Hardware-specific Board Support Packages] (`BSPs`).
//!   - [Architecture-specific code].
//!   - HW- and architecture-agnostic `kernel` code.
//!
//! using the [`kernel::interface`] traits.
//!
//! [Hardware-specific Board Support Packages]: bsp/index.html
//! [Architecture-specific code]: arch/index.html
//! [`kernel::interface`]: interface/index.html

#![allow(incomplete_features)]
#![feature(const_generics)]
#![feature(format_args_nl)]
#![feature(panic_info_message)]
#![feature(trait_alias)]
#![no_main]
#![no_std]

// Conditionally includes the selected `architecture` code, which provides the `_start()` function,
// the first function to run.
mod arch;

// `_start()` then calls `runtime_init::init()`, which on completion, jumps to `kernel_init()`.
mod runtime_init;

// Conditionally includes the selected `BSP` code.
mod bsp;

mod interface;
mod memory;
mod panic_wait;
mod print;

/// Early init code.
///
/// Concerned with with initializing `BSP` and `arch` parts.
///
/// # Safety
///
/// - Only a single core must be active and running this function.
/// - The init calls in this function must appear in the correct order.
unsafe fn kernel_init() -> ! {
    // Bring up device drivers first, so that eventual MMU errors can be printed.
    for i in bsp::device_drivers().iter() {
        if let Err(()) = i.init() {
            // This message will only be readable if, at the time of failure, the return value of
            // `bsp::console()` is already in functioning state.
            panic!("Error loading driver: {}", i.compatible())
        }
    }

    bsp::post_driver_init();

    println!("Booting on: {}", bsp::board_name());

    if let Err(string) = arch::mmu::init() {
        panic!("MMU: {}", string);
    }
    println!("MMU online");

    // Transition from unsafe to safe.
    kernel_main()
}

/// The main function running after the early init.
fn kernel_main() -> ! {
    use core::time::Duration;
    use interface::{console::All, time::Timer};

    bsp::virt_mem_layout().print_layout();

    println!(
        "Current privilege level: {}",
        arch::state::current_privilege_level()
    );
    println!("Exception handling state:");
    arch::state::print_exception_state();

    println!(
        "Architectural timer resolution: {} ns",
        arch::timer().resolution().as_nanos()
    );

    println!("Drivers loaded:");
    for (i, driver) in bsp::device_drivers().iter().enumerate() {
        println!("      {}. {}", i + 1, driver.compatible());
    }

    println!("Timer test, spinning for 1 second");
    arch::timer().spin_for(Duration::from_secs(1));

    let remapped_uart = unsafe { bsp::driver::PL011Uart::new(0x1FFF_1000) };
    writeln!(
        remapped_uart,
        "[     !!!    ] Writing through the remapped UART at 0x1FFF_1000"
    )
    .unwrap();

    println!("Echoing input now");
    loop {
        let c = bsp::console().read_char();
        bsp::console().write_char(c);
    }
}