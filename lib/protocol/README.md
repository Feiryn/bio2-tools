# Firmware protocol

## Basic protocol

The BI2A/BI2X firmware uses the following protocol, both from the computer to the board and from the board to the computer:

| Byte position | Size (bytes) | Description |
| --- | --- | --- |
| 0 | 1 | Start byte (0xAA) |
| 1 | 1 | Destination node. The board returns the same value in the response with the high bit set to 1 (i.e. OR with 0x80). |
| 2,3 | 2 | Command code with the high byte first (big endian). The board returns the same value in the response. |
| 4 | 1 | Sequence number. The board returns the same value in the response. It is recommanded for the computer to increment this number for each new command sent, but the board doesn't care about it. |
| 5 | 1 | Payload length (N) |
| 6..(6+N-1) | N | Payload data |
| (6+N) | 1 | Checksum (sum of all previous bytes (except 0xAA) modulo 256) |

The entire frame, except the start byte, is escaped using the following rules:
- 0xAA is escaped to 0xFF 0x55 (0x55 is ~0xAA)
- 0xFF is escaped to 0xFF 0x00 (0x00 is ~0xFF)

Available commands for the destination node 0x00 are:
- 0x0001: Ping. The payload must contain a single byte, and the board responds with the same byte + 1.
- 0x0002: Get firmware version. The payload is empty, and the board responds with a 44-byte payload containing the firmware version string.
- 0x0003: Not reversed yet.
- 0x0004: Enter flash mode.
- 0x0008: Not reversed yet. (BI2A only)
- 0x0030: Not reversed yet. (BI2A only)
- 0x0031: Not reversed yet. (BI2A only)
- 0x0041: Not reversed yet. (BI2A only)
- 0x0080: Not reversed yet. (BI2A only)

Example:

To get ping the board, we can send the following frame:

```
AA 00 00 01 00 01 01 03
```

- 0xAA: Start byte
- 0x00: Destination node
- 0x00 0x01: Command code (0x0001)
- 0x00: Sequence number
- 0x01: Payload length (1 byte)
- 0x01: Payload data (the byte to be incremented)
- 0x03: Checksum (0x00 + 0x00 + 0x01 + 0x00 + 0x01 + 0x01 = 0x03)

The board should respond with the following frame:

```
AA 80 00 01 00 01 02 84
```

## Flashing protocol

When the board is in flash mode, it uses a different protocol to receive the firmware data.

It then can't execute any of the previous commands, and only accepts the following frames.

All the commands end with 0xAC if everything is OK, or with 0xAF if there is an error.

The board also sends a 0xA0 byte after all commands to indicate that it is ready to receive the next command.

### 0xA1: Are you in flash mode?

The computer can send a simple `0xA1` to ask the board if it is in flash mode. The board should respond with `0xA1 0xAC` if it is in flash mode.

### 0xA2: Write block

This command is used to write a block of data to the flash memory. The frame format is as follows:

| Byte position | Size (bytes) | Description |
| --- | --- | --- |
| 0 | 1 | Start byte (0xA2) |
| 1 | 1 | 0x4B |
| 2 | 1 | Adress high byte XOR 0xFE. If a firmware is flashed, the address is 0x0000 - 0xFFFF, if a bio2base/bio2wrfirm is flashed, the address must be shifted by one on the right |
| 3 | 1 | Adress low byte XOR high byte |
| 4 | 1 | Address checksum (0x4B + (Adress high byte XOR 0xFE) + (Adress low byte XOR high byte)) modulo 256 |
| 5..132 | 128 | Data to write, encoded. Please take a look at [src/frame/flash_command.rs](src/frame/flash_command.rs) for more details on the encoding |
| 133 | 1 | Data checksum (sum of the 128 encoded data bytes modulo 256) |

### 0xA8: Start, finish flash and reboot

This command is used to start the flashing process after all the blocks have been sent, and to finish it.

To start the flashing process, the computer must send `0xA8 0x00 0x00 0x00 0x00`.

To end the flashing process, the computer must send `0xA8`.

To reboot the board, the computer must send `0xA8 0xA8 0xA8`.