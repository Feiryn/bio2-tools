mod tests {
    use protocol::reader::bio2_test_reader::Bio2TestReader;

    use crate::flash_state_machine::{FlashState, FlashStateMachine, FlashStateMachineError};

    #[test]
    fn test_written_data_64kb() {
        let reader = Box::new(Bio2TestReader::new());
        let firmware_data = std::fs::read("./assets/firmware_64kb.bin")
            .expect("Failed to read firmware file")
            .try_into()
            .expect("Firmware file must be exactly 0x10000 bytes (64KB)");

        // Create start machine and run it until completion
        let mut flash_machine = FlashStateMachine::new_64kb(reader, firmware_data);
        let mut error: Option<FlashStateMachineError> = None;
        while error.is_none() && flash_machine.state != FlashState::Finish {
            if let Err(e) = flash_machine.step() {
                error = Some(e);
            }
        }

        let expected_written_data =
            std::fs::read_to_string("./assets/expected_written_data_64kb.txt")
                .expect("Failed to read expected written data file");
        let written_data = flash_machine
            .reader
            .as_any()
            .downcast_ref::<Bio2TestReader>()
            .expect("Reader is not a Bio2TestReader")
            .get_written_data();

        // Compare each line and print differences if they don't match
        let expected_lines: Vec<&str> = expected_written_data.lines().collect();
        let mut differences_found = false;
        for (i, (expected, actual)) in expected_lines.iter().zip(written_data.iter()).enumerate() {
            if expected != actual {
                println!(
                    "Difference at line {}: expected '{}', got '{}'",
                    i + 1,
                    expected,
                    actual
                );
                differences_found = true;
            }
        }

        if differences_found {
            panic!("Written data does not match expected data. See differences above.");
        }
    }

    #[test]
    fn test_written_data_128kb() {
        let reader = Box::new(Bio2TestReader::new());
        let firmware_data = std::fs::read("./assets/firmware_128kb.bin")
            .expect("Failed to read firmware file")
            .try_into()
            .expect("Firmware file must be exactly 0x20000 bytes (128KB)");

        // Create start machine and run it until completion
        let mut flash_machine = FlashStateMachine::new_128kb(reader, firmware_data);
        let mut error: Option<FlashStateMachineError> = None;
        while error.is_none() && flash_machine.state != FlashState::Finish {
            if let Err(e) = flash_machine.step() {
                error = Some(e);
            }
        }

        let expected_written_data =
            std::fs::read_to_string("./assets/expected_written_data_128kb.txt")
                .expect("Failed to read expected written data file");
        let written_data = flash_machine
            .reader
            .as_any()
            .downcast_ref::<Bio2TestReader>()
            .expect("Reader is not a Bio2TestReader")
            .get_written_data();

        // Compare each line and print differences if they don't match
        let expected_lines: Vec<&str> = expected_written_data.lines().collect();
        let mut differences_found = false;
        for (i, (expected, actual)) in expected_lines.iter().zip(written_data.iter()).enumerate() {
            if expected != actual {
                println!(
                    "Difference at line {}: expected '{}', got '{}'",
                    i + 1,
                    expected,
                    actual
                );
                differences_found = true;
            }
        }

        if differences_found {
            panic!("Written data does not match expected data. See differences above.");
        }
    }
}
