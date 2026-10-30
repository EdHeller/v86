// The entry of a vCPU worker of the ARM core, when run from the source tree (a worker in Node, a module worker in a browser)
import { run_vcpu_arm } from "./vcpu_arm.js";
import * as C from "./control.js";

const node = typeof process !== "undefined" && process.versions && process.versions.node;

const start = async (init, post) => {
    const handle = { exports: null };
    try
    {
        await run_vcpu_arm(init, post, handle);
    }
    catch(error)
    {
        console.error("vCPU " + init.core + " failed: " + (error && error.stack || error));
        // release the machine's locks this core held before anyone waits for it
        try { handle.exports?.["parallel_fail"](); } catch(e) { /* the instance may be unusable */ }
        const ctrl = new Int32Array(init.control);
        Atomics.store(ctrl, C.core_word(init.core, C.ERROR), 1);
        Atomics.store(ctrl, C.core_word(init.core, C.STATUS), C.STATUS_FAILED);
        Atomics.add(ctrl, C.DOORBELL, 1);
        Atomics.notify(ctrl, C.DOORBELL);
        try { handle.exports?.["parallel_kick"](0); } catch(e) { /* the instance may be unusable */ }
        post({ "type": "error", "core": init.core, "message": String(error && error.stack || error) });
    }
};

if(node)
{
    import("node:" + "worker_threads").then(({ parentPort }) => {
        parentPort.once("message", init => start(init, message => parentPort.postMessage(message)));
    });
}
else
{
    const first = e => {
        globalThis.removeEventListener("message", first);
        start(e.data, message => globalThis.postMessage(message));
    };
    globalThis.addEventListener("message", first);
}
