#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_imports, unused_parens)]
use crate::cpu::cpu::after_block_boundary;
use crate::cpu::long_mode::{modrm_resolve64, rex_b, rex_r, read_imm8, read_imm8s, read_imm16, read_imm32s};
use crate::cpu::cpu::{task_switch_test, trigger_ud};
use crate::cpu::instructions;
use crate::cpu::global_pointers::{instruction_pointer, prefixes};
use crate::prefix;
#[allow(unused_variables)]
pub unsafe fn run(opcode: u32) {
match opcode {
    0xD8 | 0x1D8 => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_D8_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_D8_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud D8/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xD9 => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_D9_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_D9_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud D9/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0x1D9 => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_D9_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_D9_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud D9/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDA | 0x1DA => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DA_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DA_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud DA/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDB | 0x1DB => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_4_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DB_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DB_7_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            x => {
                dbg_log!("#ud DB/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDC | 0x1DC => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DC_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DC_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud DC/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDD => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_4_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_6_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr16_DD_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr16_DD_7_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            x => {
                dbg_log!("#ud DD/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0x1DD => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_4_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_6_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr32_DD_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr32_DD_7_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            x => {
                dbg_log!("#ud DD/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDE | 0x1DE => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_0_reg((modrm_byte & 7 | rex_b()));
                }
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DE_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DE_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud DE/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    0xDF | 0x1DF => {
        let modrm_byte = match read_imm8() { Ok(o) => o, Err(()) => return };
        match modrm_byte >> 3 & 7 {
            0 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_0_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_0_reg((modrm_byte & 7 | rex_b()));
                }
                after_block_boundary();
            },
            1 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_1_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_1_reg((modrm_byte & 7 | rex_b()));
                }
            },
            2 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_2_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_2_reg((modrm_byte & 7 | rex_b()));
                }
            },
            3 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_3_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_3_reg((modrm_byte & 7 | rex_b()));
                }
            },
            4 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_4_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_4_reg((modrm_byte & 7 | rex_b()));
                }
            },
            5 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_5_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_5_reg((modrm_byte & 7 | rex_b()));
                }
            },
            6 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_6_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_6_reg((modrm_byte & 7 | rex_b()));
                }
            },
            7 => {
                if !task_switch_test() {
                    return;
                }
                if modrm_byte < 0xC0 {
                    instructions::instr_DF_7_mem(match modrm_resolve64(modrm_byte) { Ok(a) => a, Err(()) => return });
                }
                else {
                    instructions::instr_DF_7_reg((modrm_byte & 7 | rex_b()));
                }
            },
            x => {
                dbg_log!("#ud DF/{} at {:x}", x, *instruction_pointer);
                trigger_ud();
            }
        }
    },
    _ => {
        assert!(false);
    }
}
}
