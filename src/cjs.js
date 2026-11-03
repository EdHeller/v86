/**
 * @define {boolean}
 * Overridden for production by closure compiler
 */
var DEBUG = true;

/**
 * @define {boolean}
 * Architectures as separate files: the libv86.js bundle is a 32-bit x86 machine, libv64.js adds x86_64 and
 * libv86-arm.js the ARM machine (src/browser/architectures.js). The source tree has them all built in.
 */
var ARCH_PLUGINS = false;
