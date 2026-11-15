// The VGA drawing as src/vga.js had it in JavaScript (vga_replot and vga_redraw before they moved to
// src/rust/cpu/vga_legacy.rs), for tools/profile's --check-draw: the same VGA state drawn both ways, pixel by pixel.

const VGA_BANK_SIZE = 64 * 1024;
const VGA_PIXEL_BUFFER_SIZE = 8 * VGA_BANK_SIZE;

/** @return {Int32Array} the RGBA of the screen (up to the pixel buffer's size), from the planes and palettes of `vga` */
export function reference_draw(vga)
{
    const pixel_buffer = new Uint8Array(VGA_PIXEL_BUFFER_SIZE);
    const planes = [vga.plane0, vga.plane1, vga.plane2, vga.plane3].map(p => new Uint8Array(p));
    const width = vga.virtual_width, height = vga.virtual_height;

    // vga_replot over everything
    const start = 0;
    const end = Math.min((width * height | 0xF), VGA_PIXEL_BUFFER_SIZE - 1);
    const addr_shift = vga.vga_addr_shift_count();
    const addr_substitution = ~vga.crtc_mode & 0x3;
    const shift_mode = vga.planar_mode & 0x60;
    const pel_width = vga.attribute_mode & 0x40;

    for(let pixel_addr = start; pixel_addr <= end;)
    {
        let addr = pixel_addr >>> addr_shift;
        if(addr_substitution)
        {
            let row = width ? pixel_addr / width | 0 : 0;
            const col = pixel_addr - width * row;
            switch(addr_substitution)
            {
                case 0x1: addr = (row & 0x1) << 13; row >>>= 1; break;
                case 0x2: addr = (row & 0x1) << 14; row >>>= 1; break;
                case 0x3: addr = (row & 0x3) << 13; row >>>= 2; break;
            }
            addr |= (row * width + col >>> addr_shift) + vga.start_address;
        }
        let byte0 = planes[0][addr], byte1 = planes[1][addr], byte2 = planes[2][addr], byte3 = planes[3][addr];
        const shift_loads = new Uint8Array(8);
        switch(shift_mode)
        {
            case 0x00:
                byte1 <<= 1; byte2 <<= 2; byte3 <<= 3;
                for(let i = 7; i >= 0; i--)
                {
                    shift_loads[7 - i] = byte0 >> i & 1 | byte1 >> i & 2 | byte2 >> i & 4 | byte3 >> i & 8;
                }
                break;
            case 0x20:
                shift_loads[0] = (byte0 >> 6 & 0x3) | (byte2 >> 4 & 0xC);
                shift_loads[1] = (byte0 >> 4 & 0x3) | (byte2 >> 2 & 0xC);
                shift_loads[2] = (byte0 >> 2 & 0x3) | (byte2 >> 0 & 0xC);
                shift_loads[3] = (byte0 >> 0 & 0x3) | (byte2 << 2 & 0xC);
                shift_loads[4] = (byte1 >> 6 & 0x3) | (byte3 >> 4 & 0xC);
                shift_loads[5] = (byte1 >> 4 & 0x3) | (byte3 >> 2 & 0xC);
                shift_loads[6] = (byte1 >> 2 & 0x3) | (byte3 >> 0 & 0xC);
                shift_loads[7] = (byte1 >> 0 & 0x3) | (byte3 << 2 & 0xC);
                break;
            case 0x40:
            case 0x60:
                shift_loads[0] = byte0 >> 4 & 0xF; shift_loads[1] = byte0 >> 0 & 0xF;
                shift_loads[2] = byte1 >> 4 & 0xF; shift_loads[3] = byte1 >> 0 & 0xF;
                shift_loads[4] = byte2 >> 4 & 0xF; shift_loads[5] = byte2 >> 0 & 0xF;
                shift_loads[6] = byte3 >> 4 & 0xF; shift_loads[7] = byte3 >> 0 & 0xF;
                break;
        }
        if(pel_width)
        {
            for(let i = 0, j = 0; i < 4; i++, pixel_addr++, j += 2) pixel_buffer[pixel_addr] = (shift_loads[j] << 4) | shift_loads[j + 1];
        }
        else
        {
            for(let i = 0; i < 8; i++, pixel_addr++) pixel_buffer[pixel_addr] = shift_loads[i];
        }
    }

    // vga_redraw over everything
    const out = new Int32Array(width * height);
    let mask = 0xFF, colorset = 0x00;
    if(vga.attribute_mode & 0x80)
    {
        mask &= 0xCF;
        colorset |= vga.color_select << 4 & 0x30;
    }
    // (as vga_redraw: nothing past the pixel buffer; the rest of the screen's buffer keeps what it had)
    const drawn = Math.min(out.length, VGA_PIXEL_BUFFER_SIZE);
    for(let pixel_addr = 0; pixel_addr < drawn; pixel_addr++)
    {
        let color256;
        if(vga.attribute_mode & 0x40)
        {
            color256 = (pixel_buffer[pixel_addr] & mask) | colorset;
        }
        else
        {
            const color16 = pixel_buffer[pixel_addr] & vga.color_plane_enable;
            color256 = (vga.dac_map[color16] & mask & 0x3F) | colorset | (vga.color_select << 4 & 0xC0);
        }
        const color = vga.vga256_palette[color256];
        out[pixel_addr] = color & 0xFF00 | color << 16 | color >> 16 | 0xFF000000;
    }
    return out;
}
