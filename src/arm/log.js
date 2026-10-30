// The little of src/log.js and src/const.js the ARM machine's modules use (so that it stands alone)
export const LOG_CPU = 0x2;
export function dbg_log(message, level) { if(typeof globalThis.V86_ARM_DEBUG !== "undefined" && globalThis.V86_ARM_DEBUG) console.log(message); }
export function dbg_assert(condition, message) { if(!condition) throw new Error("assertion failed" + (message ? ": " + message : "")); }
