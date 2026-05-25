mod flash_state_machine;
#[cfg(test)]
mod test_written_data;

use std::path::PathBuf;

use clap::{Parser, ValueHint};
use reader::{
    bi2a_test_reader::Bi2aTestReader,
    bio2_reader::Bio2Reader,
    bio2_serial_reader::{Bio2SerialFirmware, Bio2SerialReader},
};

use crate::flash_state_machine::{FlashState, FlashStateMachine, FlashStateMachineError};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input firmware file to flash
    #[arg(value_hint = ValueHint::FilePath)]
    firmware_path: PathBuf,

    /// COM port to use for flashing (e.g. COM5 on Windows, /dev/ttyUSB0 on Linux)
    #[arg(value_hint = ValueHint::Other)]
    com_port: PathBuf,

    /// Force flashing even if the firmware file does not look valid
    #[arg(long)]
    force_flash_of_unknown_firmware_that_may_brick_the_device: bool,

    /// Emulate the flashing process by sending commands to an internal emulator instead of a real device. In this case, the COM port argument is ignored.
    #[arg(long)]
    emulate_bio2: bool,

    /// Timeout in milliseconds for serial communication. Default is 500ms.
    #[arg(long)]
    serial_timeout_ms: Option<u64>,
}

fn main() {
    let args = Args::parse();

    // Load firmware
    let firmware_data: [u8; 0x10000] = std::fs::read(&args.firmware_path).expect("Failed to read firmware file")
        .try_into()
        .expect("Firmware file must be exactly 0x10000 bytes (64KB), but was {} bytes. 128KB firmware files are not supported yet by this tool.");

    // Check if it looks like a valid firmware
    if firmware_data[0..6] != [0x52, 0x58, 0x55, 0x31, 0x0D, 0x06] {
        eprintln!(
            "Warning: Firmware file does not start with expected header bytes. It may not be a valid Bio2 firmware"
        );
        if !args.force_flash_of_unknown_firmware_that_may_brick_the_device {
            eprintln!(
                "Aborting flashing process. Use --force-flash-of-unknown-firmware-that-may-brick-the-device to flash anyway, but be aware that flashing an invalid firmware may brick your device."
            );
            std::process::exit(1);
        }
    }

    // Open the serial port
    let reader: Box<dyn Bio2Reader> = if args.emulate_bio2 {
        Box::new(Bi2aTestReader::new())
    } else {
        Box::new(
            Bio2SerialReader::new(
                Bio2SerialFirmware::Bi2a,
                args.com_port.to_str().unwrap().to_string(),
                args.serial_timeout_ms,
            )
            .expect("Failed to open serial port"),
        )
    };

    // Create start machine and run it until completion
    let mut flash_machine = FlashStateMachine::new_64kb(reader, firmware_data);
    let mut error: Option<FlashStateMachineError> = None;
    while error.is_none() && flash_machine.state != FlashState::Finish {
        if let Err(e) = flash_machine.step() {
            error = Some(e);
        }
    }

    // todo: add check of the board version to avoid flashing incompatible firmware versions that may brick the device

    if let Some(e) = error {
        eprintln!("Flashing process failed with error: {}", e);
        std::process::exit(1);
    } else {
        println!("Flashing process completed successfully!");
    }
}
