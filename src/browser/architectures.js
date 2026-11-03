// The architectures a page can run (cpu_type): libv86.js is a 32-bit x86 PC; libv64.js (src/arch/x86_64.js) adds
// x86_64 and libv86-arm.js (src/arch/arm.js) the ARM virt machine. Each registers itself here, on the global
// V86Architectures, so that the files can be loaded in any order before new V86(). Their names are quoted: the
// files are compiled separately. In the source tree (ARCH_PLUGINS false) every architecture is built in.

const REGISTRY = "V86Architectures";

/**
 * @param {string} name "x86_64" or "arm"
 * @param {!Object} descriptor
 */
export function register_architecture(name, descriptor)
{
    const registry = globalThis[REGISTRY] = globalThis[REGISTRY] || {};
    registry[name] = descriptor;
}

/**
 * @param {string} name
 * @return {Object} the descriptor, or null when the page has not loaded it
 */
export function get_architecture(name)
{
    const registry = globalThis[REGISTRY];
    return registry && registry[name] || null;
}

/** Whether x86_64 is there: libv64.js loaded, or the source tree */
export function has_x86_64()
{
    return !ARCH_PLUGINS || !!get_architecture("x86_64");
}

/**
 * The cpu_type of new V86() options: "x86", "x86_64", "arm64" or "arm32". Without the option: x86_64 when
 * libv64.js is loaded (as v86 behaved before the split), else x86.
 * @param {!Object} options
 * @return {string}
 */
export function resolve_cpu_type(options)
{
    const cpu_type = options["cpu_type"] || (has_x86_64() ? "x86_64" : "x86");
    if(!["x86", "x86_64", "arm64", "arm32"].includes(cpu_type))
    {
        throw new Error("cpu_type: \"x86\", \"x86_64\", \"arm64\" or \"arm32\", got " + JSON.stringify(cpu_type));
    }
    if(cpu_type === "x86_64" && !has_x86_64())
    {
        throw new Error("cpu_type \"x86_64\" needs libv64.js: load it next to libv86.js (<script src=\"libv64.js\">)");
    }
    return cpu_type;
}

/**
 * The ARM architecture: from libv86-arm.js, or in the source tree its module
 * @return {!Promise<!Object>}
 */
export async function load_arm()
{
    let arch = get_architecture("arm");
    if(!arch && !ARCH_PLUGINS)
    {
        // (a computed path: the bundle never resolves it)
        const path = "../arch/" + "arm.js";
        await import(path);
        arch = get_architecture("arm");
    }
    if(!arch)
    {
        throw new Error("cpu_type \"arm64\" and \"arm32\" need libv86-arm.js: load it next to libv86.js (<script src=\"libv86-arm.js\">)");
    }
    return arch;
}
