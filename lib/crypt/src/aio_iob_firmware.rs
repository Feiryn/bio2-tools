use crate::ac_lz_inflate::AcLzInflate;

pub fn decrypt_aio_iob_firmware(firmware: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(firmware.len());
    result.extend_from_slice(firmware);

    // Decrypt the bytes
    let mut key = 0u8;
    for byte in &mut result {
        *byte ^= key;
        key = !((*byte >> 1) | ((*byte & 1) << 7));
    }

    // Decompress the firmware
    AcLzInflate::decompress(&result)
}
