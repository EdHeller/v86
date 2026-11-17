// QEMU's fw_cfg data port (0x511; SeaBIOS reads its items byte by byte, about 700 reads a boot). The item selected
// at 0x510 is made by src/cpu.js (the machine's tables, option ROMs, the kernel of -kernel) and handed over here
// (fw_cfg_select); its bytes are then read without leaving the core.

static mut VALUE: Vec<u8> = Vec::new();
static mut POINTER: usize = 0;
static mut PRESENT: bool = false;

/// Room for the selected item's `length` bytes (filled by JavaScript), read from its start
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn fw_cfg_select(length: u32) -> u32 {
    PRESENT = true;
    VALUE.clear();
    VALUE.resize(length as usize, 0);
    POINTER = 0;
    VALUE.as_mut_ptr() as u32
}

/// The next byte of the selected item (0 past its end)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn fw_cfg_read() -> i32 {
    match VALUE.get(POINTER) {
        Some(&b) => {
            POINTER += 1;
            b as i32
        },
        None => 0,
    }
}

#[inline]
pub unsafe fn serves(port: i32) -> bool { port == 0x511 && PRESENT && !super::parallel::WORKER }
