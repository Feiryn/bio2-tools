use std::path::PathBuf;

use bi2a_protocol::{
    command_response::CommandResponse, command_response_decoder::CommandResponseDecoder,
    host_command::HostCommand,
};
use bi2x_protocol::{command::Bi2xCommand, frame::Bi2xFrame, response::Bi2xResponse};
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

fn read_bi2a_response(reader: &mut Box<dyn Bio2Reader>) -> Result<CommandResponse, Option<String>> {
    let mut response_decoder = CommandResponseDecoder::new();

    while let Some(byte) = reader.read_byte().map_err(|_| None)? {
        match response_decoder.update_get_raw(byte) {
            Ok(Some((response, _bytes))) => return Ok(response),
            Ok(None) => {}
            Err(e) => return Err(Some(format!("Failed to decode response: {}", e))),
        }
    }

    Err(None)
}

fn read_bi2x_response(reader: &mut Box<dyn Bio2Reader>) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();

    loop {
        match reader.read_byte() {
            Ok(Some(byte)) => response.push(byte),
            _ => break,
        }
    }

    if response.is_empty() {
        Err("No response received".to_string())
    } else {
        Ok(response)
    }
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
            Some(1000),
        )
        .expect("Failed to open serial port"),
    );

    reader
        .write_bytes(&HostCommand::Version.to_frame(0x00))
        .expect("Failed to send version command");

    match read_bi2a_response(&mut reader) {
        Ok(response) => match response {
            CommandResponse::Version {
                sequence_number,
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
                    std::process::exit(1);
                }

                println!(
                    "Received version response from the device: {} v{}.{}.{} - {} {}",
                    name, major, minor, patch, date, time
                );
            }
            _ => {
                println!("Received unexpected response: {:?}", response);
                std::process::exit(1);
            }
        },
        Err(Some(e)) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        Err(None) => {
            println!("Device did not respond to BI2A version command. Trying as BI2X...");
        }
    }

    drop(reader);

    let mut reader: Box<dyn Bio2Reader> = Box::new(
        Bio2SerialReader::new(
            Bio2SerialFirmware::Bi2x,
            args.com_port
                .to_str()
                .expect("Invalid COM port path")
                .to_string(),
            Some(1000),
        )
        .expect("Failed to open serial port"),
    );

    let mut tries = 0;
    while tries < 2 {
        tries += 1;

        reader
            .write_bytes(
                &Bi2xCommand::Init
                    .to_frame(0x01)
                    .to_bytes(true)
                    .expect("Failed to serialize Init command"),
            )
            .expect("Failed to send init command");

        match read_bi2x_response(&mut reader) {
            Ok(response) => {
                let frame =
                    Bi2xFrame::parse(&response, true).expect("Failed to parse init response frame");
                let response =
                    Bi2xResponse::from_frame(frame).expect("Failed to parse init response");
                if response.len() != 1 {
                    println!("Received unexpected response length: {}", response.len());
                    std::process::exit(1);
                }

                match response[0] {
                    Bi2xResponse::InitializationResult { success } => {
                        if success {
                            println!(
                                "Device responded to BI2X init command. Device is likely a BI2X device."
                            );
                        } else {
                            println!(
                                "Device responded to BI2X init command but indicated initialization failure, probably already initialized. Ignoring and trying to get firmware version anyway..."
                            );
                        }
                    }
                    _ => {
                        println!("Received unexpected response: {:?}", response[0]);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                println!("Device did not respond to BI2X init command: {}", e);
            }
        }
    }

    reader
        .write_bytes(
            &Bi2xCommand::GetFirmwareVersion
                .to_frame(0x02)
                .to_bytes(true)
                .expect("Failed to serialize GetFirmwareVersion command"),
        )
        .expect("Failed to send GetFirmwareVersion command");

    match read_bi2x_response(&mut reader) {
        Ok(response) => {
            let frame = Bi2xFrame::parse(&response, true)
                .expect("Failed to parse GetFirmwareVersion response frame");
            let response = Bi2xResponse::from_frame(frame)
                .expect("Failed to parse GetFirmwareVersion response");
            if response.len() != 1 {
                println!("Received unexpected response length: {}", response.len());
                std::process::exit(1);
            }

            match &response[0] {
                Bi2xResponse::FirmwareVersion {
                    success: _,
                    major,
                    minor,
                    patch,
                    name,
                    timestamp_ms,
                } => {
                    let timestamp =
                        std::time::UNIX_EPOCH + std::time::Duration::from_millis(*timestamp_ms);
                    let datetime: chrono::DateTime<chrono::Utc> = timestamp.into();

                    println!(
                        "Received firmware version response from the device: {} v{}.{}.{} - {}",
                        name,
                        major,
                        minor,
                        patch,
                        datetime.format("%Y-%m-%d %H:%M:%S")
                    );
                }
                _ => {
                    println!("Received unexpected response: {:?}", response[0]);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            println!(
                "Device did not respond to GetFirmwareVersion command. Unable to identify device: {}",
                e
            );
            std::process::exit(1);
        }
    }
}
