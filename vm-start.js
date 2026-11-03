"use strict";
var p9_connected = false;
var show = false;
var running = true;
var debug = false;
var mouselock = true;

function fitVMScreen() {
    var emulator = window.emulator;
    if (!emulator) return;
    var box = document.getElementById("screen_box");
    var scr = document.getElementById("screen_container");
    emulator.screen_set_scale(1, 1);
    var r = scr.getBoundingClientRect();
    if (!r.width || !r.height) return;      // panel hidden: nothing to fit
    var n = Math.min((box.clientWidth - 6) / r.width, (box.clientHeight - 6) / r.height);
    emulator.screen_set_scale(n, n);
}

//very large function.
function startVM(button1)
{
    //fix the iframe focus /issues/1645
    // Clicking the VM screen gives it the keyboard; with capture on it also locks the mouse
    var screenBox = document.getElementById("screen_container");
    screenBox.addEventListener("mousedown", function()
    {
        var el = document.activeElement;
        if(el && el !== document.body) el.blur();
        window.focus();
    }, true);

    screenBox.addEventListener("click", function()
    {
        if(!window.emulator) return;
        window.emulator.keyboard_set_status(true);
        if(mouselock && !document.pointerLockElement) window.emulator.lock_mouse();
    });

    //VM started change our button text and function.
    //note: remove listner doesn't remove the current function for some reason firefox 158
    button1.innerHTML = "Stop";
    button1.onclick = null;
    button1.addEventListener('click', toggleRun);

    var emulator = window.emulator = new V86({
        wasm_path: "/v86.wasm",
        memory_size: 512 * 1024 * 1024,
        vga_memory_size: 16 * 1024 * 1024,      //8-16MB
        screen_container: document.getElementById("screen_container"),
        bios:     { url: "/bios/seabios.bin" },
        vga_bios: { url: "/bios/vgabios.bin" },   // required: the VESA boot mode comes from this BIOS
        bzimage:  { url: "/images/v86-bash-env-bzImage", size: 4080128 },
        initrd:   { url: "/images/v86-bash-env-rootfs.cpio.zstd", size: 62899485 },
        net_devices: [
            { type: "ne2k",   relay_url: "wss://relay.widgetry.org/" },  // internet (DHCP from the relay)
            { type: "virtio", relay_url: "inbrowser" },                  // other v86 pages in this browser
        ],
        filesystem: {
            basefs: "",
            baseurl: "",
        },
        serial_console: {
            type: "xtermjs",
            container: document.getElementById("serial_terminal"),
            xterm_lib: V86GraphemeTerminal,
        },
        autostart: false,
    });

    window.emulator.add_listener("screen-set-size", fitVMScreen);
    new ResizeObserver(fitVMScreen).observe(document.getElementById("screen_box"));

    // ── Info panel (as on copy.sh/v86) ──────────────────────────────
    var info_last_tick = 0;
    var info_running_time = 0;
    var info_last_counter = 0;
    var info_total_instructions = 0;
    var info_timer = null;
    var info_fs_read = 0, info_fs_written = 0;
    var info_net_received = 0, info_net_transmitted = 0;

    function info_format_time(s)
    {
        var pad = function(n) { return String(n).padStart(2, "0"); };
        if(s < 60) return s + "s";
        if(s < 3600) return (s / 60 | 0) + "m " + pad(s % 60) + "s";
        return (s / 3600 | 0) + "h " + pad((s / 60 | 0) % 60) + "m " + pad(s % 60) + "s";
    }

    function info_update()
    {
        var now = Date.now();
        var counter = emulator.get_instruction_counter();
        if(counter < info_last_counter)
        {
            info_last_counter -= 0x100000000;   // 32-bit wrap-around
        }
        var ips = counter - info_last_counter;
        var dt = now - info_last_tick;
        info_last_counter = counter;
        info_total_instructions += ips;
        if(!dt) return;

        info_running_time += dt;
        info_last_tick = now;
        document.getElementById("speed").textContent = (ips / 1000 / dt).toFixed(1);
        document.getElementById("avg_speed").textContent = (info_total_instructions / 1000 / info_running_time).toFixed(1);
        document.getElementById("running_time").textContent = info_format_time(info_running_time / 1000 | 0);
    }

    emulator.add_listener("emulator-started", function()
    {
        info_last_tick = Date.now();
        info_timer = setInterval(info_update, 1000);
    });

    emulator.add_listener("emulator-stopped", function()
    {
        info_update();
        clearInterval(info_timer);
    });

    emulator.add_listener("9p-read-end", function(args)
    {
        info_fs_read += args[1];
        document.getElementById("info_filesystem").style.display = "block";
        document.getElementById("info_filesystem_bytes_read").textContent = info_fs_read;
    });

    emulator.add_listener("9p-write-end", function(args)
    {
        info_fs_written += args[1];
        document.getElementById("info_filesystem").style.display = "block";
        document.getElementById("info_filesystem_bytes_written").textContent = info_fs_written;
    });

    emulator.add_listener("eth-receive-end", function(args)
    {
        info_net_received += args[0];
        document.getElementById("info_network").style.display = "block";
        document.getElementById("info_network_bytes_received").textContent = info_net_received;
    });

    emulator.add_listener("eth-transmit-end", function(args)
    {
        info_net_transmitted += args[0];
        document.getElementById("info_network").style.display = "block";
        document.getElementById("info_network_bytes_transmitted").textContent = info_net_transmitted;
    });

    emulator.add_listener("mouse-enable", function(is_enabled)
    {
        document.getElementById("info_mouse_enabled").textContent = is_enabled ? "Yes" : "No";
    });

    emulator.add_listener("screen-set-size", function(args)
    {
        var w = args[0], h = args[1], bpp = args[2];
        document.getElementById("info_res").textContent = w + "x" + h + (bpp ? "x" + bpp : "");
        document.getElementById("info_vga_mode").textContent = bpp ? "Graphical" : "Text";
    });
    //end info panel

    // ── Download progress bar ───────────────────────────────────────
    document.getElementById("vm_loading").style.display = "block";

    emulator.add_listener("download-progress", function(e)
    {
        var text = document.getElementById("vm_loading_text");
        var bar = document.getElementById("vm_loading_bar");
        var name = e.file_name.split("?")[0].split("/").pop();

        if(name.endsWith(".wasm"))
        {
            text.textContent = "Loading the emulator…";
            bar.removeAttribute("value");          // moving "busy" bar
            return;
        }

        var line = "Downloading " + name + " [" + (e.file_index + 1) + "/" + e.file_count + "]";
        if(e.total)
        {
            var percent = Math.min(100, Math.floor(e.loaded / e.total * 100));
            bar.value = percent;
            line += "  " + percent + "%  (" + (e.loaded / 1048576).toFixed(1) +
                " of " + (e.total / 1048576).toFixed(1) + " MB)";
        }
        else
        {
            bar.removeAttribute("value");          // size unknown: busy bar
            line += "  " + (e.loaded / 1048576).toFixed(1) + " MB";
        }
        text.textContent = line;
    });

    emulator.add_listener("download-error", function(e)
    {
        document.getElementById("vm_loading_text").textContent =
            "Downloading " + e.file_name + " failed. Check your connection and reload the page.";
    });

    emulator.add_listener("emulator-loaded", function()
    {
        document.getElementById("vm_loading").style.display = "none";
    });
    //end progress bar

    emulator.add_listener("emulator-ready", async function()
    {
        console.log("Starting up...");
        emulator.run();
        document.getElementById('screen_container').style.border = ".3vh solid green";
    });

    emulator.add_listener("9p-attach", function()
    {
        if(p9_connected) return;
        p9_connected = true;
        console.log("9p file system ready.");

        const container = document.getElementById("p9");
        if(!container)
        {
            console.warn('Element with id "p9" not found.');
            return;
        }

        // Hidden file input, kept out of the tab order so it can't take focus
        const fileInput = document.createElement("input");
        fileInput.id = "fileinput";
        fileInput.type = "file";
        fileInput.tabIndex = -1;
        fileInput.style.position = "absolute";
        fileInput.style.left = "-9999px";

        const openButton = document.createElement("button");
        openButton.textContent = "Add file to VM";
        openButton.addEventListener("click", () => fileInput.click());

        fileInput.addEventListener("change", async (event) =>
        {
            try
            {
                const file = event.target.files && event.target.files[0];
                if(!file) return;
                const bytes = new Uint8Array(await file.arrayBuffer());   // binary-safe
                await emulator.create_file(file.name, bytes);
                event.target.value = "";            // allow re-adding the same file
                openButton.focus();
            }
            catch(err)
            {
                console.error("File upload failed:", err);
            }
        });

        const lbl = document.createElement("label");
        lbl.htmlFor = "downloadfile";
        lbl.textContent = "Get file from emulator:";
        lbl.style.color = "black";

        // Use an existing #downloadfile input if the page has one, else create it
        let downloadfile = document.getElementById("downloadfile");
        if(!downloadfile)
        {
            downloadfile = document.createElement("input");
            downloadfile.id = "downloadfile";
            downloadfile.type = "text";
        }

        downloadfile.addEventListener("keydown", async (e) =>
        {
            if(e.isComposing || e.key !== "Enter") return;
            const name = downloadfile.value.trim();   // path inside /mnt/9p, e.g. "dir/a.txt"
            if(!name) return;
            try
            {
                const bytes = await emulator.read_file(name);
                download(bytes, name.split("/").pop(), "application/octet-stream");
            }
            catch(err)
            {
                alert("Not found in /mnt/9p: " + name);
            }
            downloadfile.blur();
            document.getElementById("screen_container").focus();
        });

        container.appendChild(openButton);
        container.appendChild(document.createElement("br"));
        container.appendChild(lbl);
        container.appendChild(fileInput);
        container.appendChild(downloadfile);
    });
}

function download(bytes, filename, type) {
    const url = URL.createObjectURL(new Blob([bytes], { type }));
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => { a.remove(); URL.revokeObjectURL(url); }, 0);
}
