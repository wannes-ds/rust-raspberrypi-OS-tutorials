// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2018-2020 Andre Richter <andre.o.richter@gmail.com>

// Rust embedded logo for `make doc`.
#![doc(html_logo_url = "https://git.io/JeGIp")]

//! The `kernel` binary.

#![feature(format_args_nl)]
#![no_main]
#![no_std]
#![feature(alloc_error_handler)]
#![feature(llvm_asm)]

extern crate alloc;

use core::borrow::BorrowMut;
use libkernel::{
    bsp,
    bsp::device_driver::{
        Mailbox, Message, PropertyTag, PropertyTagPowerState, PropertyTagTemperature,
    },
    cpu, driver, exception, info, memory, state, time, warn,
};
use linked_list_allocator::LockedHeap;

#[global_allocator]
static GLOBAL_ALLOCATOR: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
fn foo(_: core::alloc::Layout) -> ! {
    panic!("TITSUP")
}

/// Early init code.
///
/// # Safety
///
/// - Only a single core must be active and running this function.
/// - The init calls in this function must appear in the correct order:
///     - Virtual memory must be activated before the device drivers.
///       - Without it, any atomic operations, e.g. the yet-to-be-introduced spinlocks in the device
///         drivers (which currently employ IRQSafeNullLocks instead of spinlocks), will fail to
///         work on the RPi SoCs.
#[no_mangle]
unsafe fn kernel_init() -> ! {
    use driver::interface::DriverManager;
    use memory::mmu::interface::MMU;

    exception::handling_init();

    if let Err(string) = memory::mmu::mmu().init() {
        panic!("MMU: {}", string);
    }

    GLOBAL_ALLOCATOR.lock().init(0x0020_0000, 4 * 1024 * 1024);

    for i in bsp::driver::driver_manager().all_device_drivers().iter() {
        if i.init().is_err() {
            panic!("Error loading driver: {}", i.compatible())
        }
    }
    bsp::driver::driver_manager().post_device_driver_init();
    // println! is usable from here on.

    // Let device drivers register and enable their handlers with the interrupt controller.
    for i in bsp::driver::driver_manager().all_device_drivers() {
        if let Err(msg) = i.register_and_enable_irq_handler() {
            warn!("Error registering IRQ handler: {}", msg);
        }
    }

    // Unmask interrupts on the boot CPU core.
    exception::asynchronous::local_irq_unmask();

    // Announce conclusion of the kernel_init() phase.
    state::state_manager().transition_to_single_core_main();

    // Transition from unsafe to safe.
    kernel_main()
}

/// The main function running after the early init.
unsafe fn kernel_main() -> ! {
    use driver::interface::DriverManager;
    use exception::asynchronous::interface::IRQManager;

    info!("Booting on: {}", bsp::board_name());

    info!("MMU online. Special regions:");
    bsp::memory::mmu::virt_mem_layout().print_layout();

    let (_, privilege_level) = exception::current_privilege_level();
    info!("Current privilege level: {}", privilege_level);

    info!("Exception handling state:");
    exception::asynchronous::print_state();

    info!(
        "Architectural timer resolution: {} ns",
        time::time_manager().resolution().as_nanos()
    );

    info!("Drivers loaded:");
    for (i, driver) in bsp::driver::driver_manager()
        .all_device_drivers()
        .iter()
        .enumerate()
    {
        info!("      {}. {}", i + 1, driver.compatible());
    }

    info!("Registered IRQ handlers:");
    bsp::exception::asynchronous::irq_manager().print_handler();

    let tmb = &mut PropertyTagTemperature {
        temperature_id: PropertyTagTemperature::TEMPERATURE_ID,
        value: 0,
    };
    let mut tag = PropertyTag::new(0x00030006, tmb);
    let mut temp = Message::new(tag.borrow_mut());

    match bsp::MAILBOX.send(Mailbox::BCM_MAILBOX_PROP_CHANNEL, &mut temp) {
        Ok(tres) => {
            info!("Temp is {:.2} C", tres.value / 1000);
        }
        _ => {}
    }

    info!("USB CORE {}", bsp::DWHCI);

    info!("Echoing input now");
    cpu::wait_forever();
}
