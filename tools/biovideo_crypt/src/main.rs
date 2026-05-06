use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand, ValueHint};
use crypt::{BiovideoEncryptionFiles, decrypt_biovideo, encrypt_biovideo};

/// Encrypt or decrypt a biovideo file.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decrypt an encrypted biovideo file into bio2base and firmware files
    Decrypt {
        /// Input encrypted biovideo file (must be exactly 0x20000 bytes)
        #[arg(value_hint = ValueHint::FilePath)]
        input: PathBuf,

        /// Output path for the bio2base file
        #[arg(value_hint = ValueHint::FilePath)]
        bio2base: PathBuf,

        /// Output path for the firmware file
        #[arg(value_hint = ValueHint::FilePath)]
        firmware: PathBuf,
    },
    /// Encrypt bio2base and firmware files into a biovideo file
    Encrypt {
        /// Input bio2base file (must be exactly 0x20000 bytes)
        #[arg(value_hint = ValueHint::FilePath)]
        bio2base: PathBuf,

        /// Input firmware file (must be exactly 0x10000 bytes)
        #[arg(value_hint = ValueHint::FilePath)]
        firmware: PathBuf,

        /// Output path for the encrypted biovideo file
        #[arg(value_hint = ValueHint::FilePath)]
        output: PathBuf,
    },
}

fn main() {
    let args = Args::parse();

    match args.command {
        Command::Decrypt {
            input,
            bio2base,
            firmware,
        } => {
            let input_data = read_file_exact::<0x20000>(&input);

            println!("Decrypting '{}' ...", input.display());

            let result = decrypt_biovideo(input_data);

            write_file(&bio2base, &result.bio2base);
            write_file(&firmware, &result.firmware);

            println!(
                "Done. bio2base written to '{}', firmware written to '{}'.",
                bio2base.display(),
                firmware.display()
            );
        }
        Command::Encrypt {
            bio2base,
            firmware,
            output,
        } => {
            let bio2base_data = read_file_exact::<0x20000>(&bio2base);
            let firmware_data = read_file_exact::<0x10000>(&firmware);

            println!(
                "Encrypting '{}' and '{}' ...",
                bio2base.display(),
                firmware.display()
            );

            let result = encrypt_biovideo(BiovideoEncryptionFiles {
                bio2base: bio2base_data,
                firmware: firmware_data,
            });

            write_file(&output, &result);

            println!("Done. Output written to '{}'.", output.display());
        }
    }
}

fn read_file_exact<const N: usize>(path: &PathBuf) -> [u8; N] {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error reading '{}': {}", path.display(), e);
            process::exit(1);
        }
    };
    match data.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            eprintln!(
                "Error: '{}' has the wrong size (expected {} bytes).",
                path.display(),
                N
            );
            process::exit(1);
        }
    }
}

fn write_file(path: &PathBuf, data: &[u8]) {
    if let Err(e) = fs::write(path, data) {
        eprintln!("Error writing '{}': {}", path.display(), e);
        process::exit(1);
    }
}
