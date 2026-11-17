// virtio-snd (src/virtio_snd.js): one output PCM stream (S16 or S32, mono or stereo, 5512 to 48000 Hz). The control
// queue's requests (information on jacks, streams and channel maps; set parameters, prepare, start, stop, release),
// the transmit queue's periods, completed as the machine clock plays them (the timer pass: timer), the input queue
// refused. The samples go to JavaScript (virtio_snd.js's output: the speaker).

use crate::cpu::cpu::js;
use crate::cpu::virtio_dev::*;

const R_JACK_INFO: u32 = 1;
const R_PCM_INFO: u32 = 0x0100;
const R_PCM_SET_PARAMS: u32 = 0x0101;
const R_PCM_PREPARE: u32 = 0x0102;
const R_PCM_RELEASE: u32 = 0x0103;
const R_PCM_START: u32 = 0x0104;
const R_PCM_STOP: u32 = 0x0105;
const R_CHMAP_INFO: u32 = 0x0200;
const S_OK: u32 = 0x8000;
const S_BAD_MSG: u32 = 0x8001;
const S_NOT_SUPP: u32 = 0x8002;
const FMT_S16: u8 = 5;
const FMT_S32: u8 = 17;
const RATES: [u32; 8] = [5512, 8000, 11025, 16000, 22050, 32000, 44100, 48000];
const RATE_FIRST: u8 = 1;
const RATE_LAST: u8 = 7;
const CHMAP_FL: u8 = 3;
const CHMAP_FR: u8 = 4;
const QUEUE_CONTROL: usize = 0;
const QUEUE_TX: usize = 2;
const QUEUE_RX: usize = 3;
const PCM_INFO_BYTES: u32 = 32;
const CHMAP_INFO_BYTES: u32 = 24;

/// what js::virtio_model_event tells of virtio-snd: samples (kind 0 + format: 0 s16, 1 s32; channels; the rate in
/// the high bits), the stream started (1: its rate as data)
pub const EVENT_START: i32 = 1;

#[derive(Clone, Copy)]
struct Params {
    channels: u8,
    format: u8,
    rate: u32,
    buffer_bytes: u32,
    period_bytes: u32,
    frame_bytes: u32,
}

#[derive(Default)]
struct Stream {
    params: Option<Params>,
    prepared: bool,
    running: bool,
    started: f64,
    played: f64,
    queue: Vec<(Chain, Vec<u8>)>,
}

static mut DEVICE: usize = usize::MAX;
static mut STREAM: Option<Stream> = None;

#[allow(static_mut_refs)]
unsafe fn stream() -> &'static mut Stream { STREAM.get_or_insert_with(Stream::default) }

#[no_mangle]
pub unsafe fn virtio_snd_attach(d: i32) {
    DEVICE = d as usize;
    STREAM = Some(Stream::default());
    MODEL[d as usize] = MODEL_SND;
}

#[no_mangle]
pub unsafe fn virtio_snd_reset() { STREAM = Some(Stream::default()); }

fn status(code: u32) -> Vec<u8> { code.to_le_bytes().to_vec() }

unsafe fn request(request: &[u8]) -> Vec<u8> {
    if request.len() < 4 {
        return status(S_BAD_MSG);
    }
    let word = |at: usize| if request.len() >= at + 4 { u32::from_le_bytes(request[at..at + 4].try_into().unwrap()) as i64 } else { -1 };
    let code = word(0) as u32;
    if code == R_JACK_INFO || code == R_PCM_INFO || code == R_CHMAP_INFO {
        let (start, count, size) = (word(4), word(8), word(12));
        let items = if code == R_PCM_INFO || code == R_CHMAP_INFO { 1 } else { 0 };
        let minimum = if code == R_PCM_INFO { PCM_INFO_BYTES } else { CHMAP_INFO_BYTES } as i64;
        if start < 0 || count < 0 || size < minimum || start + count > items {
            return status(S_BAD_MSG);
        }
        let (count, size) = (count as usize, size as usize);
        let mut response = vec![0u8; 4 + count * size];
        response[..4].copy_from_slice(&S_OK.to_le_bytes());
        for i in 0..count {
            let item = &mut response[4 + i * size..4 + (i + 1) * size];
            if code == R_PCM_INFO {
                item[8..12].copy_from_slice(&((1u32 << FMT_S16) | (1u32 << FMT_S32)).to_le_bytes());
                let mut rates = 0u32;
                for r in RATE_FIRST..=RATE_LAST {
                    rates |= 1 << r;
                }
                item[16..20].copy_from_slice(&rates.to_le_bytes());
                item[24] = 0;
                item[25] = 1;
                item[26] = 2;
            }
            else {
                item[4] = 0;
                item[5] = 2;
                item[6] = CHMAP_FL;
                item[7] = CHMAP_FR;
            }
        }
        return response;
    }
    if word(4) != 0 {
        return status(S_BAD_MSG);
    }
    let s = stream();
    match code {
        R_PCM_SET_PARAMS => {
            if request.len() < 24 {
                return status(S_BAD_MSG);
            }
            let (buffer_bytes, period_bytes) = (word(8) as u32, word(12) as u32);
            let (channels, format, rate) = (request[20], request[21], request[22]);
            if s.running || channels < 1 || channels > 2 || format != FMT_S16 && format != FMT_S32 || rate < RATE_FIRST
                || rate > RATE_LAST || period_bytes == 0 || buffer_bytes < period_bytes
            {
                return status(S_NOT_SUPP);
            }
            s.params = Some(Params {
                channels,
                format,
                rate: RATES[rate as usize],
                buffer_bytes,
                period_bytes,
                frame_bytes: channels as u32 * if format == FMT_S16 { 2 } else { 4 },
            });
            s.prepared = false;
            status(S_OK)
        },
        R_PCM_PREPARE => {
            if s.params.is_none() || s.running {
                return status(S_BAD_MSG);
            }
            s.prepared = true;
            s.played = 0.0;
            status(S_OK)
        },
        R_PCM_START => {
            if !s.prepared || s.running {
                return status(S_BAD_MSG);
            }
            s.running = true;
            s.started = js::microtick();
            s.played = 0.0;
            let rate = s.params.unwrap().rate.to_le_bytes();
            js::virtio_model_event(DEVICE as i32, EVENT_START, rate.as_ptr() as u32, 4);
            status(S_OK)
        },
        R_PCM_STOP => {
            if !s.running {
                return status(S_BAD_MSG);
            }
            play(js::microtick());
            stream().running = false;
            status(S_OK)
        },
        R_PCM_RELEASE => {
            if s.running {
                return status(S_BAD_MSG);
            }
            let n = s.queue.len();
            complete(n);
            stream().prepared = false;
            status(S_OK)
        },
        _ => status(S_NOT_SUPP),
    }
}

pub unsafe fn notify(d: usize, queue: usize) {
    match queue {
        QUEUE_CONTROL => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut message = vec![0u8; c.length_readable as usize];
                c.read(&mut message);
                let reply = request(&message);
                c.write(&reply);
                push(d, queue, &c);
            }
            flush(d, queue);
        },
        QUEUE_TX => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut header = [0u8; 4];
                c.read(&mut header);
                let mut data = vec![0u8; (c.length_readable as usize).saturating_sub(4)];
                c.read(&mut data);
                stream().queue.push((c, data));
            }
            let s = stream();
            if s.running {
                play(js::microtick());
            }
            else if !s.prepared {
                let n = s.queue.len();
                complete(n);
            }
        },
        QUEUE_RX => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut reply = [0u8; 8];
                reply[..4].copy_from_slice(&S_BAD_MSG.to_le_bytes());
                c.write(&reply);
                push(d, queue, &c);
            }
            flush(d, queue);
        },
        _ => {},
    }
}

/// The periods the clock has played by `now`: completed
unsafe fn play(now: f64) {
    let s = stream();
    let Some(p) = s.params else { return };
    if !s.running {
        return;
    }
    let due = ((now - s.started) / 1000.0 * p.rate as f64).floor() * p.frame_bytes as f64;
    let (mut count, mut played) = (0, s.played);
    while count < s.queue.len() && played + s.queue[count].1.len() as f64 <= due {
        played += s.queue[count].1.len() as f64;
        count += 1;
    }
    if count != 0 {
        complete(count);
    }
}

/// The first `count` periods done: their samples to JavaScript, their replies (the bytes still queued)
unsafe fn complete(count: usize) {
    let d = DEVICE;
    let s = stream();
    let done: Vec<(Chain, Vec<u8>)> = s.queue.drain(..count.min(s.queue.len())).collect();
    let left: usize = s.queue.iter().map(|q| q.1.len()).sum();
    for (mut c, bytes) in done {
        let s = stream();
        s.played += bytes.len() as f64;
        if let Some(p) = s.params {
            if !bytes.is_empty() {
                let kind = (p.format == FMT_S32) as i32 | (p.channels as i32) << 1 | (p.rate as i32) << 4;
                js::virtio_model_event(d as i32, kind << 1, bytes.as_ptr() as u32, bytes.len() as i32);
            }
        }
        let mut reply = [0u8; 8];
        reply[..4].copy_from_slice(&S_OK.to_le_bytes());
        reply[4..].copy_from_slice(&(left as u32).to_le_bytes());
        c.write(&reply);
        push(d, QUEUE_TX, &c);
    }
    flush(d, QUEUE_TX);
}

/// From the core's timer pass: milliseconds until the next period is due
pub unsafe fn timer(now: f64) -> f64 {
    if DEVICE == usize::MAX {
        return 100.0;
    }
    let s = stream();
    if !s.running || s.queue.is_empty() {
        return 100.0;
    }
    play(now);
    let s = stream();
    let Some(p) = s.params else { return 100.0 };
    if s.queue.is_empty() {
        return 100.0;
    }
    let next = (s.played + s.queue[0].1.len() as f64) / p.frame_bytes as f64 / p.rate as f64 * 1000.0 + s.started;
    (next - now).ceil().clamp(1.0, 100.0)
}

/// The stream for a snapshot, into virtio_scratch as f64: has params, channels, format, rate, buffer bytes, period
/// bytes, prepared, running, started, played, then the queued periods' descriptor heads; returns how many numbers
#[no_mangle]
pub unsafe fn virtio_snd_save() -> i32 {
    let s = stream();
    let mut v: Vec<f64> = Vec::new();
    match s.params {
        Some(p) => v.extend([1.0, p.channels as f64, p.format as f64, p.rate as f64, p.buffer_bytes as f64, p.period_bytes as f64]),
        None => v.extend([0.0; 6]),
    }
    v.extend([s.prepared as u8 as f64, s.running as u8 as f64, s.started, s.played]);
    v.extend(s.queue.iter().map(|q| q.0.head as f64));
    let at = virtio_scratch(8 * v.len() as u32) as *mut f64;
    std::ptr::copy_nonoverlapping(v.as_ptr(), at, v.len());
    v.len() as i32
}

/// The stream of a snapshot (`count` numbers in virtio_scratch, as virtio_snd_save); the queued periods are read
/// again from their descriptors
#[no_mangle]
pub unsafe fn virtio_snd_load(count: i32) {
    let v = std::slice::from_raw_parts(scratch() as *const f64, count as usize).to_vec();
    let mut s = Stream::default();
    if v[0] != 0.0 {
        let (channels, format) = (v[1] as u8, v[2] as u8);
        s.params = Some(Params {
            channels,
            format,
            rate: v[3] as u32,
            buffer_bytes: v[4] as u32,
            period_bytes: v[5] as u32,
            frame_bytes: channels as u32 * if format == FMT_S16 { 2 } else { 4 },
        });
    }
    s.prepared = v[6] != 0.0;
    s.running = v[7] != 0.0;
    s.started = v[8];
    s.played = v[9];
    for &head in &v[10..] {
        let mut c = chain_at(DEVICE, QUEUE_TX, head as i32);
        let mut header = [0u8; 4];
        c.read(&mut header);
        let mut data = vec![0u8; (c.length_readable as usize).saturating_sub(4)];
        c.read(&mut data);
        s.queue.push((c, data));
    }
    STREAM = Some(s);
}
