//! The ARM core of the virt machine (an AArch64 CPU: interpreter, page tier, GICv3, timers, PSCI, the
//! bus), taken from the v64 project and built as its own crate (build/v86-arm64.wasm), next to the x86
//! core of the main crate. The two cores share nothing at build time.

#[macro_use]
#[allow(dead_code)]
mod dbg;

// The state block each core declares its fields in (state!)
#[macro_use]
mod state_block;

pub mod aarch64;
use aarch64::isa_hooks as isa;

pub mod parallel;
mod extended;
mod core_stats;
#[allow(dead_code)]
mod jitrt;
#[allow(dead_code)]
mod leb;
#[allow(dead_code)]
mod page;
#[allow(dead_code)]
mod pagegen;
#[allow(dead_code)]
mod wasmgen;
mod zstd;
