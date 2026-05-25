use std::path::PathBuf;

use bi2a_protocol::host_command::HostCommand;
use clap::{Parser, ValueHint};
use reader::{
    bio2_reader::Bio2Reader,
    bio2_serial_reader::{Bio2SerialFirmware, Bio2SerialReader},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// COM port to use for flashing (e.g. COM5 on Windows, /dev/ttyUSB0 on Linux)
    #[arg(value_hint = ValueHint::Other)]
    com_port: PathBuf,
}

fn main() {
    let args = Args::parse();

    let mut reader: Box<dyn Bio2Reader> = Box::new(
        Bio2SerialReader::new(
            Bio2SerialFirmware::Bi2a,
            args.com_port
                .to_str()
                .expect("Invalid COM port path")
                .to_string(),
            Some(500),
        )
        .expect("Failed to open serial port"),
    );

    let mut curr_addr: u32 = 0xfff80000;
    let end_addr: u32 = 0xffffffff;
    let mut dump = Vec::new();

    while curr_addr <= end_addr - 0x20 {
        let command = &HostCommand::Custom {
            addr: 0x01,
            command_code: 0x0002,
            payload: vec![
                curr_addr as u8,
                (curr_addr >> 8) as u8,
                (curr_addr >> 16) as u8,
                (curr_addr >> 24) as u8,
            ],
        }
        .to_frame(0x00);

        println!("Sending read command: {:02X?}", command);

        match reader.write_bytes(command) {
            Ok(_) => {
                let mut response = Vec::new();
                let mut escape_next = false;
                let mut read_error = false;
                let expected_size = 0x8 + 0x20;
                let mut curr_size = 0;
                while curr_size < expected_size {
                    match reader.read_byte() {
                        Ok(Some(byte)) => {
                            if escape_next {
                                response.push(!byte);
                                escape_next = false;
                            } else if byte == 0xFF {
                                escape_next = true;
                            } else {
                                response.push(byte);
                            }
                            curr_size = response.len();
                        }
                        Ok(None) => {
                            break;
                        }
                        Err(e) => {
                            eprintln!("Failed to read response from device: {}", e);
                            read_error = true;
                            break;
                        }
                    }
                }
                if read_error {
                    break;
                }
                if response.len() < 7 || response[6] != 0x20 {
                    eprintln!(
                        "Received unexpected response for address {:08X}: {:02X?}",
                        curr_addr, response
                    );
                    break;
                }
                dump.extend_from_slice(&response[7..(0x20 + 7)]);
                println!("Dumped {:08X} - {:08X}", curr_addr, curr_addr + 0x20);
                curr_addr += 0x20;
            }
            Err(e) => {
                eprintln!("Failed to send version command: {}", e);
                break;
            }
        }
    }

    std::fs::write("flash_dump.bin", dump).expect("Failed to write flash dump to file");
}
