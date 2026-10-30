#!/usr/bin/env node
// The virtio-net device of the ARM machine: Alpine aarch64 configures eth0 and pings a host that answers ARP and ICMP echo
// requests in JavaScript (the frames are net0-send / net0-receive on the machine's bus, as for v86's network adapters).

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "../..");
const { A64Machine } = await import(path.join(root, "src/arm/machine.js"));
const { iso_files } = await import(path.join(root, "src/arm/iso.js"));
const iso_path = process.env.A64_ISO || path.join(root, "build/a64/alpine-virt-aarch64.iso");
const b = fs.readFileSync(iso_path);
const image = b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength);
const files = iso_files(image);
const take = name => { const [offset, length] = files.get(name); return image.slice(offset, offset + length); };

const HOST_MAC = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02], HOST_IP = [10, 0, 2, 2];
const machine = await A64Machine.create({
    wasm: fs.readFileSync(path.join(root, "build/v86-arm64.wasm")),
    kernel: take("BOOT/VMLINUZ_VIRT"), initrd: take("BOOT/INITRAMFS_VIRT"),
    cmdline: "console=ttyAMA0 modules=loop,squashfs,sd-mod,virtio-blk,virtio-mmio,virtio_blk,virtio_mmio,virtio_net,isofs",
    memory_size: 512 << 20, cpu_cores: 1, disks: [{ buffer: image, readonly: true }], net_device: {},
});
const checksum = bytes => {
    let sum = 0;
    for(let i = 0; i < bytes.length; i += 2) sum += (bytes[i] << 8) + (bytes[i + 1] || 0);
    while(sum > 0xFFFF) sum = (sum & 0xFFFF) + (sum >> 16);
    return ~sum & 0xFFFF;
};
let sent = 0, answered = 0;
machine.bus.register("net0-send", frame => {
    sent++;
    const type = frame[12] << 8 | frame[13];
    if(type === 0x0806 && frame[21] === 1 && frame.slice(38, 42).join() === HOST_IP.join())
    {
        // ARP request for the host: reply
        const reply = new Uint8Array(42);
        reply.set(frame.slice(6, 12), 0); reply.set(HOST_MAC, 6); reply.set([8, 6, 0, 1, 8, 0, 6, 4, 0, 2], 12);
        reply.set(HOST_MAC, 22); reply.set(HOST_IP, 28); reply.set(frame.slice(22, 28), 32); reply.set(frame.slice(28, 32), 38);
        answered++;
        machine.bus.send("net0-receive", reply);
    }
    else if(type === 0x0800 && frame[23] === 1 && frame[34] === 8)
    {
        // an echo request to the host: reply
        const reply = new Uint8Array(frame);
        reply.set(frame.slice(6, 12), 0); reply.set(HOST_MAC, 6);
        reply.set(frame.slice(30, 34), 26); reply.set(frame.slice(26, 30), 30);
        reply[22] = 64; reply[24] = reply[25] = 0;
        const ihl = (frame[14] & 15) * 4;
        const header = reply.subarray(14, 14 + ihl);
        header[10] = header[11] = 0;
        const hc = checksum(header); header[10] = hc >> 8; header[11] = hc & 255;
        reply[34] = 0; reply[36] = reply[37] = 0;
        const icmp = reply.subarray(14 + ihl);
        const cc = checksum(icmp); reply[36] = cc >> 8; reply[37] = cc & 255;
        answered++;
        machine.bus.send("net0-receive", reply);
    }
});
let serial = "";
machine.add_listener("serial0-output-byte", c => { serial += String.fromCharCode(c); });
const start = Date.now();
machine.run();
const wait_for = (re, seconds) => new Promise((resolve, reject) => {
    const from = Date.now(), mark = serial.length;
    const t = setInterval(() => {
        if(re.test(serial.slice(mark))) { clearInterval(t); resolve(); }
        else if(Date.now() - from > seconds * 1000) { clearInterval(t); reject(new Error("timeout waiting for " + re + "\n" + serial.slice(-600))); }
    }, 200);
});
try
{
    await wait_for(/localhost login:/, +process.env.A64_SECS || 600);
    machine.serial0_send("root\n");
    await wait_for(/localhost:~# /, 60);
    machine.serial0_send("ip addr add 10.0.2.15/24 dev eth0; ip link set eth0 up; ip route add default via 10.0.2.2; echo link-ready\n");
    await wait_for(/\nlink-ready/, 30);
    machine.serial0_send("ping -c 3 -W 5 10.0.2.2; echo ping-done\n");
    await wait_for(/\nping-done/, 60);
    if(!/3 packets transmitted, 3 packets received/.test(serial)) throw new Error("ping failed:\n" + serial.slice(-600));
    console.log("ok: ping through virtio-net (%d frames sent by the guest, %d answered), %ss", sent, answered, ((Date.now() - start) / 1000).toFixed(0));
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    process.exit(1);
}
