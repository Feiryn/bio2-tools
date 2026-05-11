use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand, ValueHint};

/// Tool to generate a Motorola S-record (MOT) file for flashing the BIO2 board.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    BI2A {
        /// Path to the data file (32kb)
        #[arg(value_hint = ValueHint::FilePath)]
        data_file: PathBuf,

        /// Path to the bio2base file (128kb)
        #[arg(value_hint = ValueHint::FilePath)]
        bio2base_file: PathBuf,

        /// Path to the firmware file (64kb)
        #[arg(value_hint = ValueHint::FilePath)]
        firmware_file: PathBuf,

        // Output MOT file path
        #[arg(value_hint = ValueHint::FilePath)]
        output_file: PathBuf,
    },
    BI2X {
        /// Path to the data file (32kb)
        #[arg(value_hint = ValueHint::FilePath)]
        data_file: PathBuf,

        /// Path to the bio2wrfirm file (128kb)
        #[arg(value_hint = ValueHint::FilePath)]
        bio2wrfirm_file: PathBuf,

        /// Path to the bi2x firmware file (128kb)
        #[arg(value_hint = ValueHint::FilePath)]
        firmware_file: PathBuf,

        // Output MOT file path
        #[arg(value_hint = ValueHint::FilePath)]
        output_file: PathBuf,
    },
}

fn load_file(path: &PathBuf, expected_size: usize) -> Vec<u8> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() != expected_size {
                eprintln!(
                    "Error: File {} must be exactly {} bytes, but is {} bytes",
                    path.display(),
                    expected_size,
                    bytes.len()
                );
                process::exit(1);
            }
            bytes
        }
        Err(e) => {
            eprintln!("Error reading file {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}

fn calculate_checksum(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    0xFFu8.wrapping_sub(sum)
}

fn push_data_records(mot_content: &mut String, start_address: u32, data: &[u8]) {
    for i in (0..data.len()).step_by(16) {
        let chunk = &data[i..i + 16];
        mot_content.push_str(&format!(
            "S3{:02X}{:08X}{}{:02X}\r\n",
            chunk.len() + 5,
            start_address + i as u32,
            hex::encode(chunk).to_uppercase(),
            calculate_checksum(
                &[
                    chunk.len() as u8 + 5,
                    ((start_address + i as u32) >> 24) as u8,
                    ((start_address + i as u32) >> 16) as u8,
                    ((start_address + i as u32) >> 8) as u8,
                    (start_address + i as u32) as u8
                ]
                .iter()
                .chain(chunk.iter())
                .cloned()
                .collect::<Vec<u8>>()
            )
        ));
    }
}

fn main() {
    let args = Args::parse();

    let (data, bio2_firmware, bi2a_x_firmware, output_file) = match args.command {
        Command::BI2A {
            data_file,
            bio2base_file,
            firmware_file,
            output_file,
        } => {
            let data = load_file(&data_file, 0x8000);
            let bio2_firmware = load_file(&bio2base_file, 0x20000);
            let mut bi2a_x_firmware = load_file(&firmware_file, 0x10000);
            bi2a_x_firmware.extend(bi2a_x_firmware.to_owned()); // Bank A and B

            (data, bio2_firmware, bi2a_x_firmware, output_file)
        }
        Command::BI2X {
            data_file,
            bio2wrfirm_file: bio2wrfirm,
            firmware_file: kernel_file,
            output_file,
        } => {
            let data = load_file(&data_file, 0x8000);
            let bio2_firmware = load_file(&bio2wrfirm, 0x20000);
            let bi2a_x_firmware = load_file(&kernel_file, 0x20000);

            (data, bio2_firmware, bi2a_x_firmware, output_file)
        }
    };

    // Padding
    let padding = vec![0xFF; 0x40000];

    // MOT file content
    let mut mot_content = String::new();

    // Header
    let header = "Created by bio2_tools";
    let header_bytes = header.as_bytes();
    let header_bytes_sum: u8 = header_bytes.iter().fold(0, |acc, &b| acc.wrapping_add(b));
    mot_content.push_str(&format!(
        "S0{:02X}0000{}{:02X}\r\n",
        header.len() + 3,
        hex::encode(header).to_uppercase(),
        0xFFu8.wrapping_sub(header_bytes_sum.wrapping_add(header.len() as u8 + 3))
    ));

    // Data records
    push_data_records(&mut mot_content, 0x00100000, &data);
    push_data_records(&mut mot_content, 0xFFF80000, &padding);
    push_data_records(&mut mot_content, 0xFFFC0000, &bio2_firmware);
    push_data_records(&mut mot_content, 0xFFFE0000, &bi2a_x_firmware);

    // End of file record
    mot_content.push_str("S70500000000FA\r\n");

    // Write to output file
    match fs::write(&output_file, mot_content) {
        Ok(_) => println!(
            "MOT file generated successfully at {}",
            output_file.display()
        ),
        Err(e) => {
            eprintln!("Error writing MOT file {}: {}", output_file.display(), e);
            process::exit(1);
        }
    }
}
