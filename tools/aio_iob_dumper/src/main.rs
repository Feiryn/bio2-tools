use std::path::PathBuf;

use clap::{Parser, ValueHint};
use crypt::{compute_crc, decrypt_aio_iob_firmware};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input libaio_iob.dll file
    #[arg(value_hint = ValueHint::FilePath)]
    aio_iob_file: PathBuf,

    // Output directory for the dumped firmware files
    #[arg(value_hint = ValueHint::DirPath)]
    output_dir: PathBuf,
}

// bio2wrfirm
const BIO2WRFIRM_START_BYTES: [u8; 6] = [0xFF, 0xE3, 0x6A, 0x03, 0x62, 0x02];
const BIO2WRFIRM_END_BYTES: [u8; 7] = [0x77, 0x4D, 0xBC, 0x9A, 0xFC, 0x4E, 0x00];

// bi2x firmware
const BI2X_FIRMWARE_START_BYTES: [u8; 6] = [0xBF, 0x72, 0x8E, 0x86, 0x64, 0x6A];
const BI2X_FIRMWARE_END_BYTES: [u8; 6] = [0xF6, 0xC7, 0x20, 0x0F, 0x78, 0xFF];

// bi2x kernel
const BI2X_KERNEL_START_BYTES: [u8; 6] = [0xFF, 0x0D, 0x7F, 0xFC, 0xFE, 0x7F];
const BI2X_KERNEL_END_BYTES: [u8; 5] = [0xC7, 0x20, 0x1F, 0x70, 0xFF];

fn find_firmware(file_content: &[u8], start_bytes: &[u8], end_bytes: &[u8]) -> Option<Vec<u8>> {
    let start_pos = file_content
        .windows(start_bytes.len())
        .position(|window| window == start_bytes)?;
    let end_pos = file_content[start_pos..]
        .windows(end_bytes.len())
        .position(|window| window == end_bytes)?
        + start_pos
        + end_bytes.len();
    Some(file_content[start_pos..end_pos].to_vec())
}

fn main() {
    let args = Args::parse();

    // Open the file and read its contents
    let file_content = std::fs::read(&args.aio_iob_file).expect("Failed to read the file");

    // Search for the firmware byte sequences
    let bio2wrfirm = find_firmware(
        &file_content,
        &BIO2WRFIRM_START_BYTES,
        &BIO2WRFIRM_END_BYTES,
    );
    let bi2x_firmware = find_firmware(
        &file_content,
        &BI2X_FIRMWARE_START_BYTES,
        &BI2X_FIRMWARE_END_BYTES,
    );
    let bi2x_kernel = find_firmware(
        &file_content,
        &BI2X_KERNEL_START_BYTES,
        &BI2X_KERNEL_END_BYTES,
    );

    // Panic if any of the firmware sequences were not found
    let bio2wrfirm = bio2wrfirm.expect("bio2wrfirm byte sequence not found in the file. The file may be too recent for the tool. Please report this issue to the developer.");
    let bi2x_firmware = bi2x_firmware.expect("bi2x firmware byte sequence not found in the file. The file may be too recent for the tool. Please report this issue to the developer.");
    let bi2x_kernel = bi2x_kernel.expect("bi2x kernel byte sequence not found in the file. The file may be too recent for the tool. Please report this issue to the developer.");

    // Decrypt and build bio2wrfirm
    let mut bio2wrfirm_buffer = [0xFF; 0x20000];
    let bio2wrfirm = decrypt_aio_iob_firmware(&bio2wrfirm);
    bio2wrfirm_buffer[..bio2wrfirm.len()].copy_from_slice(&bio2wrfirm);
    bio2wrfirm_buffer[bio2wrfirm.len()..(bio2wrfirm.len() + bi2x_kernel.len())]
        .copy_from_slice(&bi2x_kernel); // Append the encrypted kernel at the end
    let crc = compute_crc(&bio2wrfirm_buffer[2..]);
    bio2wrfirm_buffer[0] = crc[0];
    bio2wrfirm_buffer[1] = crc[1];

    // Decrypt bi2x firmware
    let bi2x_firmware = crypt::decrypt_aio_iob_firmware(&bi2x_firmware);

    // Decrypt bi2x kernel
    let bi2x_kernel = crypt::decrypt_aio_iob_firmware(&bi2x_kernel);

    // Save the decrypted firmware files
    std::fs::write(args.output_dir.join("bio2wrfirm.bin"), bio2wrfirm_buffer)
        .expect("Failed to write bio2wrfirm.bin");
    std::fs::write(args.output_dir.join("bi2x_firmware.bin"), bi2x_firmware)
        .expect("Failed to write bi2x_firmware.bin");
    std::fs::write(args.output_dir.join("bi2x_kernel.bin"), bi2x_kernel)
        .expect("Failed to write bi2x_kernel.bin");

    println!(
        "Successfully dumped and decrypted the firmware files to {}",
        args.output_dir.display()
    );
}
