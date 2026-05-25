// Yeah this is some kind of customized random generator
fn scramble_byte(seed: u32, byte: u8) -> (u32, u8) {
    if byte & 0xAA == 0xAA {
        return (seed, byte);
    }

    let mut scrambled = seed as u32;
    scrambled = scrambled.wrapping_mul(0x41c64e6d).wrapping_add(0x3039);
    let seed = scrambled;

    let mask: u32 = if byte & 0x80 != 0 { 0x55 } else { 0x7F };

    return (seed, ((scrambled & mask) as u8) ^ byte);
}

pub fn decrypt_bi2x_payload(sequence_number: u8, payload: &[u8], is_from_bio2: bool) -> Vec<u8> {
    let xor = if is_from_bio2 { 0xAA } else { 0x55 };
    let mut seed = sequence_number ^ xor;
    let mut decrypted = Vec::new();
    for &byte in payload {
        let (new_seed, decrypted_byte) = scramble_byte(seed as u32, byte);
        decrypted.push(decrypted_byte);
        seed = new_seed as u8;
    }
    decrypted
}

pub fn encrypt_bi2x_payload(sequence_number: u8, payload: &[u8], is_for_bio2: bool) -> Vec<u8> {
    let xor = if is_for_bio2 { 0x55 } else { 0xAA };
    let mut seed = sequence_number ^ xor;
    let mut encrypted = Vec::new();
    for &byte in payload {
        let (new_seed, encrypted_byte) = scramble_byte(seed as u32, byte);
        encrypted.push(encrypted_byte);
        seed = new_seed as u8;
    }
    encrypted
}
