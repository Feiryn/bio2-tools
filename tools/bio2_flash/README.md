# BIO2 Flasher

This tool is used to flash a firmware to a BIO2 board.

It currently only supports flashing from BI2A to BI2A itself. If you try to flash something else, it may brick the device, so use it with caution.

If you wish to flash from BI2A to BI2X or vice versa, you can use a MOT file (see the main README for more details).

## Usage

```
bio2_flash <firmware_file> <serial_port>
```

Available options:
- `--force-flash-of-unknown-firmware-that-may-brick-the-device`: Force flash a firmware that is not recognized by the tool. This may brick the device if the firmware is not compatible with the board, so use it with caution.
- `--emulate-bio2`: Emulate the BIO2 protocol without actually flashing the firmware to the board. This can be used for testing the tool without risking bricking the device.
- `--serial-timeout-ms`: Set the serial timeout in milliseconds. Default is 500 ms.