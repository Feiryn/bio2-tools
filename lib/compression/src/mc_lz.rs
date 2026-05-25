// This one also have been generated using Copilot
// Wtf is this

const WIN: usize = 85;

fn next_slot(slot: usize) -> usize {
    if slot + 1 == WIN { 0 } else { slot + 1 }
}

fn compare_slots(ring: &[u8; WIN], src: usize, cur: usize) -> usize {
    let mut len = 1usize;
    while len < 4 && ring[(src + len) % WIN] == ring[(cur + len) % WIN] {
        len += 1;
    }
    len
}

fn remove_slot(buckets: &mut [Vec<usize>], slot_bucket: &[u8; WIN], slot: usize) {
    let bucket = slot_bucket[slot] as usize;
    if let Some(pos) = buckets[bucket].iter().position(|&idx| idx == slot) {
        buckets[bucket].remove(pos);
    }
}

fn insert_slot(
    ring: &[u8; WIN],
    slot_bucket: &mut [u8; WIN],
    buckets: &mut [Vec<usize>],
    slot: usize,
) -> (usize, usize) {
    let bucket = ring[slot] as usize;
    slot_bucket[slot] = bucket as u8;

    let chain = &mut buckets[bucket];
    let mut best_len = 0usize;
    let mut best_src = 0usize;

    for idx in 0..chain.len() {
        let src = chain[idx];
        let len = compare_slots(ring, src, slot);
        if len > best_len {
            best_len = len;
            best_src = src;
            if len == 4 {
                chain[idx] = slot;
                return (best_len, best_src);
            }
        }
    }

    chain.push(slot);
    (best_len, best_src)
}

// ── inflate ──────────────────────────────────────────────────────────────────

/// Decompress a raw MC_LZ_DEFLATE stream.
///
/// `out_limit` is the exact uncompressed length from the frame VarLen field;
/// decompression stops when that many bytes have been written.
pub fn inflate(data: &[u8], out_limit: usize) -> Vec<u8> {
    let mut window = [0u8; WIN];
    let mut wpos = WIN - 4;
    let mut out = Vec::with_capacity(out_limit);
    let mut pos = 0usize;

    'outer: while pos < data.len() && out.len() < out_limit {
        let flags = data[pos];
        pos += 1;
        let mut bp = 0u32;

        // Tokens may start at bits 0..6. Bit 7 is only used as the
        // trailing half of a two-bit token that starts at bit 6.
        while bp < 7 && out.len() < out_limit {
            if (flags >> bp) & 1 == 0 {
                // Bit 0 → literal
                if pos >= data.len() {
                    break 'outer;
                }
                let b = data[pos];
                pos += 1;
                out.push(b);
                window[wpos] = b;
                wpos = next_slot(wpos);
                bp += 1;
            } else {
                bp += 1;
                if bp >= 8 {
                    // Lone leading 1 at bit 7 — padding, end of block.
                    break;
                }
                if (flags >> bp) & 1 == 0 {
                    // Bits 10 → back-reference
                    if pos >= data.len() {
                        break 'outer;
                    }
                    let t = data[pos] as usize;
                    pos += 1;
                    let actual = if t >= 0xAB { t - 1 } else { t };
                    let length = actual / WIN + 2; // 2..=4
                    let mut src = actual % WIN; // absolute source slot 0..=84
                    for _ in 0..length {
                        if out.len() >= out_limit {
                            break;
                        }
                        let b = window[src];
                        src = next_slot(src);
                        out.push(b);
                        window[wpos] = b;
                        wpos = next_slot(wpos);
                    }
                } else {
                    // Bits 11 → 0xAA literal
                    out.push(0xAA);
                    window[wpos] = 0xAA;
                    wpos = next_slot(wpos);
                }
                bp += 1;
            }
        }
    }

    out
}

// ── deflate ───────────────────────────────────────────────────────────────────

/// Compress `data` for the MC_LZ_DEFLATE codec.
///
/// Returns the byte sequence to pass to `MC_LZ_DEFLATE::PutChar`.
/// Store the original (`data.len()`) as `out_limit` for `inflate`.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut ring = [0u8; WIN];
    let mut slot_bucket = [0u8; WIN];
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); 256];
    let mut lookahead_end = WIN - 4;
    let mut state_flag = 0usize;
    let mut pending = 0usize;
    let mut input_pos = 0usize;
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + data.len() / 4 + 1);

    let mut flags: u8 = 0;
    let mut bit_pos: u32 = 0;
    let mut flags_idx: usize = 0;
    out.push(0); // first flags-byte placeholder

    while input_pos < data.len() && pending < 4 {
        ring[(lookahead_end + pending) % WIN] = data[input_pos];
        pending += 1;
        input_pos += 1;
    }

    for delta in 1..=4 {
        let slot = (lookahead_end + WIN - delta) % WIN;
        insert_slot(&ring, &mut slot_bucket, &mut buckets, slot);
    }
    let (mut match_len, mut match_src) =
        insert_slot(&ring, &mut slot_bucket, &mut buckets, lookahead_end);

    while pending > 0 {
        // The original encoder leaves flag bit 7 unused and starts a new
        // block once seven bits have been consumed.
        if bit_pos >= 7 {
            out[flags_idx] = flags;
            flags = 0;
            bit_pos = 0;
            flags_idx = out.len();
            out.push(0);
        }

        let emit_len = match_len.min(pending);
        if emit_len >= 2 {
            flags |= 0b01 << bit_pos;
            bit_pos += 2;

            let actual = (emit_len - 2) * WIN + match_src;
            let token = if actual > 0xA9 {
                (actual + 1) as u8
            } else {
                actual as u8
            };
            out.push(token);
        } else {
            let byte = ring[lookahead_end];
            if byte == 0xAA {
                flags |= 0b11 << bit_pos;
                bit_pos += 2;
            } else {
                bit_pos += 1;
                out.push(byte);
            }
        }

        for _ in 0..emit_len.max(1) {
            remove_slot(&mut buckets, &slot_bucket, state_flag);

            if input_pos < data.len() {
                ring[state_flag] = data[input_pos];
                slot_bucket[state_flag] = ring[state_flag];
                input_pos += 1;
                pending += 1;
            }

            state_flag = next_slot(state_flag);
            lookahead_end = next_slot(lookahead_end);
            pending -= 1;

            if pending > 0 {
                (match_len, match_src) =
                    insert_slot(&ring, &mut slot_bucket, &mut buckets, lookahead_end);
            }
        }
    }

    // Commit the final flags block, or drop an empty trailing placeholder.
    if bit_pos > 0 {
        out[flags_idx] = flags;
    } else {
        out.pop();
    }

    out
}

// ── helpers ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression() {
        let input = vec![
            0x00, 0x13, 0x02, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x32, 0x00, 0x00, 0xa4, 0x1c, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x2d, 0x00, 0x00, 0xe5, 0x1c, 0x00, 0x00, 0xf0,
            0x2d, 0x00, 0x00, 0x00, 0xff, 0x08, 0x13, 0x00, 0x00, 0x09, 0x13, 0x01, 0x00, 0x0a,
            0x13, 0x02, 0x00, 0x0b, 0x13, 0x03, 0x00, 0x05, 0x13, 0x15, 0x00, 0x06, 0x13, 0x17,
            0x00, 0x14, 0x0b, 0x22, 0x00, 0x13, 0x0b, 0x23, 0x00, 0x18, 0x0c, 0x24, 0x00, 0x17,
            0x0c,
        ];
        let expected_output = vec![
            0x08, 0x00, 0x13, 0x02, 0xf8, 0xf8, 0x32, 0x51, 0x50, 0xa4, 0x1c, 0xff, 0x50, 0x24,
            0xf0, 0x2d, 0x50, 0xe5, 0x5d, 0x41, 0xba, 0x00, 0xff, 0x08, 0x13, 0x50, 0x20, 0x09,
            0x13, 0x01, 0x00, 0x0a, 0xa7, 0x00, 0x0b, 0x13, 0x03, 0x00, 0x05, 0x13, 0x15, 0x00,
            0x00, 0x06, 0x13, 0x17, 0x00, 0x14, 0x0b, 0x02, 0x22, 0x51, 0x0b, 0x23, 0x00, 0x18,
            0x00, 0x0c, 0x24, 0x00, 0x17, 0x0c,
        ];

        let compressed = deflate(&input);

        println!("Compressed: {:02x?}", compressed);
        println!("Expected:   {:02x?}", expected_output);

        assert_eq!(compressed, expected_output);
    }

    #[test]
    fn test_decompression() {
        let compressed = vec![
            0x08, 0x00, 0x13, 0x02, 0xf8, 0xf8, 0x32, 0x51, 0x50, 0xa4, 0x1c, 0xff, 0x50, 0x24,
            0xf0, 0x2d, 0x50, 0xe5, 0x5d, 0x41, 0xba, 0x00, 0xff, 0x08, 0x13, 0x50, 0x20, 0x09,
            0x13, 0x01, 0x00, 0x0a, 0xa7, 0x00, 0x0b, 0x13, 0x03, 0x00, 0x05, 0x13, 0x15, 0x00,
            0x00, 0x06, 0x13, 0x17, 0x00, 0x14, 0x0b, 0x02, 0x22, 0x51, 0x0b, 0x23, 0x00, 0x18,
            0x00, 0x0c, 0x24, 0x00, 0x17, 0x0c,
        ];
        let expected_output = vec![
            0x00, 0x13, 0x02, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x32, 0x00, 0x00, 0xa4, 0x1c, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x2d, 0x00, 0x00, 0xe5, 0x1c, 0x00, 0x00, 0xf0,
            0x2d, 0x00, 0x00, 0x00, 0xff, 0x08, 0x13, 0x00, 0x00, 0x09, 0x13, 0x01, 0x00, 0x0a,
            0x13, 0x02, 0x00, 0x0b, 0x13, 0x03, 0x00, 0x05, 0x13, 0x15, 0x00, 0x06, 0x13, 0x17,
            0x00, 0x14, 0x0b, 0x22, 0x00, 0x13, 0x0b, 0x23, 0x00, 0x18, 0x0c, 0x24, 0x00, 0x17,
            0x0c,
        ];

        let decompressed = inflate(&compressed, expected_output.len());

        println!("Decompressed: {:02x?}", decompressed);
        println!("Expected:     {:02x?}", expected_output);

        assert_eq!(decompressed, expected_output);
    }
}
