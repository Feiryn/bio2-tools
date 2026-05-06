use std::path::PathBuf;

use clap::{Parser, ValueHint};
use protocol::{
    frame::{
        command_response::CommandResponse, command_response_decoder::CommandResponseDecoder,
        host_command::HostCommand,
    },
    reader::{bio2_reader::Bio2Reader, bio2_serial_reader::Bio2SerialReader},
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
            args.com_port
                .to_str()
                .expect("Invalid COM port path")
                .to_string(),
            Some(500),
        )
        .expect("Failed to open serial port"),
    );

    let mut response_decoder = CommandResponseDecoder::new();

    reader
        .write_bytes(&HostCommand::Version.to_frame(0x00))
        .expect("Failed to send version command");

    while let Some(byte) = reader
        .read_byte()
        .expect("Failed to read response from device")
    {
        match response_decoder.update_get_raw(byte) {
            Ok(Some((response, bytes))) => match response {
                CommandResponse::Version {
                    sequence_number,
                    revision,
                    major,
                    minor,
                    patch,
                    name,
                    date,
                    time,
                } => {
                    if sequence_number != 0x00 {
                        eprintln!(
                            "Received version response with unexpected sequence number: {}",
                            sequence_number
                        );
                        break;
                    }

                    println!("Raw version response bytes: {:02X?}", bytes);

                    println!(
                        "Received version response from the device: {} v{}.{}.{} rev {} - {} {}",
                        name, major, minor, patch, revision, date, time
                    );
                    break;
                }
                _ => {
                    println!("Received unexpected response: {:?}", response);
                    std::process::exit(1);
                }
            },
            Ok(None) => {}
            Err(e) => {
                eprintln!("Failed to decode response: {}", e);
                std::process::exit(1);
            }
        }
    }
}
