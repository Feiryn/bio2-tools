# This project

This project is a collection of tools, libraries and documentation related to the BIO2 board.

Any help is welcomed, as there is a lot to still be reversed and understood.

> [!WARNING] 
> 
> I am not responsible for any damage that may occur to your board or cabinet by using these tools.
> 
> It has not been tested on a real cabinet, on windows, and bi2x has almost not been tested at all (except that it responds to the identify command).

If you just want to flash your board and don't care about understanding how it works, just go to the [Flashing the board](#flashing-the-board) section and especially the [With MOT file and official Renesas flashing tool](#with-mot-file-and-official-renesas-flashing-tool) section.

## Tools

- [aio_iob_dumper](tools/aio_iob_dumper): A tool to dump and decrypt the firmware files contained in the `libaio_iob.dll` file.
- [bi2a_flash](tools/bi2a_flash): A tool to flash a firmware to a BI2A using the internal firmware updater.
- [bio2_flash_dumper](tools/bio2_flash_dumper): A development tool used to dump the MCU using a custom firmware injected in the board. You probably don't need this tool.
- [bio2_ident](tools/bio2_ident): A tool to identify the board and its firmware version.
- [biovideo_crypt](tools/biovideo_crypt): A tool to encrypt and decrypt a biovideo file.
- [generate_mot](tools/generate_mot): A tool to generate a MOT file that can be used to flash the board using the official Renesas flashing tools.

# BIO2

## What is it?

A BIO2 is a board used in some arcade cabinets, that handles the I/O of the cabinet.

It communicates to a computer via a serial over USB connection. The connection runs at 115200 baud, 8N1.

The board itself contains an Renesas RX621 microcontroller, flashed with a proprietary firmware.

## Firmwares

### Stock firmware

When shipped, the board is flashed with a stock firmware, that basically handles the flashing of the game firmware.

This is not easily dumpable, and actually not really useful to dump, so the process of dumping it has not been done yet.

The flashing protocol has not been fully reverse engineered yet too.

In order to flash a game firmware, you can either use the official flashing tool (`bio2updatew7.exe`), or use a [MOT file and an official flash Renesas flashing tool](#with-mot-file-and-official-renesas-flashing-tool).

### Game firmware

#### MCU memory mapping

The RX621 microcontroller has a 32-bit address space.

- `0x00000000` - `0x00017FFF`: On-chip RAM. It is used by the program ROM as a RAM space for variables, stack, etc.
- `0x00018000` - `0x00080000`: Reserved area.
- `0x00080000` - `0x000FFFFF`: Peripheral I/O registers. The RX621 has a lot of peripherals, and they are all mapped in this area. They can either be input, output, both or setting registers. The game firmware uses this area for various purposes, such as reading inputs, controlling outputs, etc.
- `0x00100000` - `0x00107FFF`: On-chip ROM (data flash). It contains ROM data used by the program ROM and can be flashed. The content of this area can be found in [`assets/data_flash.bin`](assets/data_flash.bin).
- `0x00108000` - `0x007F7FFF`: Reserved area.
- `0x007F8000` - `0x007F9FFF`: FCU-RAM. Used for flashing.
- `0x007FA000` - `0x007FBFFF`: Reserved area.
- `0x007FC000` - `0x007FC4FF`: Some other peripheral I/O registers.
- `0x007FC500` - `0x007FFBFF`: Reserved area.
- `0x007FFC00` - `0x00800000`: Some other peripheral I/O registers.
- `0x00800000` - `0x00F80000`: Reserved area.
- `0x00F80000` - `0x00FFFFFF`: On-chip ROM (program ROM) (write only). It contains the program code of the firmware and is write-only. It can be used to flash the firmware, but it cannot be read.
- `0x01000000` - `0xFEFFDFFF`: Reserved area.
- `0xFEFFE000` - `0xFEFFFFFF`: On-chip ROM (FCU firmware) (read only).
- `0xFF000000` - `0xFF7FBFFF`: Reserved area.
- `0xFF7FC000` - `0xFF7FFFFF`: On-chip ROM (user boot) (read only).
- `0xFF800000` - `0xFFF80000`: Reserved area.
- `0xFFF80000` - `0xFFFFFFFF`: On-chip ROM (program ROM) (read only). It contains the actual program code of the firmware and is read-only. This is where the board is executing code from. The following ranges are defined in the program ROM built by the constructor:
    - `0xFFF80000` - `0xFFFBFFFF`: Empty area, filled with `0xFF`.
    - `0xFFFC0000` - `0xFFFDFFFF`: [bio2base/bio2wrfirm](#bio2basebio2wrfirm).
    - `0xFFFE0000` - `0xFFFEFFFF`: [bi2a/bi2x firmware](#bi2abi2x-firmware) bank A. It is used as a backup bank if something's wrong with the main bank.
    - `0xFFFF0000` - `0xFFFFFFFF`: [bi2a/bi2x firmware](#bi2abi2x-firmware) bank B. It is the main bank used by the board.

#### bio2base/bio2wrfirm

The bio2base/bio2wrfirm firmware is the first stage bootloader of the board. The bio2base is used for BI2A and the bio2wrfirm is used for BI2X.

It runs before the bi2a/bi2x firmware. It has not been fully reverse engineered yet, but it is responsible for:
- Verifying the integrity of the bi2a/bi2x firmware using a CRC over the whole firmware, and restoring the bank A or B depending on the correct one (if A is correct and B is not, it will copy A to B, and vice versa).
- If both banks are correct, it will jump to the main firmware (bank B).

This bio2base can be found in the [biovideo file](#biovideo-file) and the bio2wrfirm can be found in the [libaio_iob.dll file](#libaio_iobdll-file).

#### bi2a/bi2x firmware

The bi2a/bi2x firmware is the main firmware of the board.

It too has not been fully reverse engineered yet, but it is responsible for:
- Initializing the board and its peripherals, such as the USB serial connection, the I/O of the cabinet, etc.
- Handling the communication with the computer via the USB serial connection and interpreting the commands sent by the computer.
- Handling the flashing of the firmware, by receiving the firmware data from the computer and writing it to the program ROM.
- Handling the I/O of the cabinet, by reading the inputs from the cabinet and controlling the outputs of the cabinet.

The bi2a firmware can be found in the [biovideo file](#biovideo-file) and the bi2x firmware can be found in the [libaio_iob.dll file](#libaio_iobdll-file).

#### Firmware sources

##### biovideo file

The biovideo file is an encrypted file that contains both the bio2base and the bi2a firmware. It can be found in the `D:\PCB\` directory of an official arcade cabinet hard drive, and is used to flash the firmware to the board.

The [biovideo_crypt](tools/biovideo_crypt) tool can be used to decrypt the biovideo file and extract the bio2base and bi2a firmware. The [README of the tool](tools/biovideo_crypt/README.md) contains more details on the encryption and decryption process.

##### libaio_iob.dll file

This DLL is used by games to update the firmware from BI2A to BI2X. Therefore, it contains the bio2wrfirm, the bi2x firmware and the bi2x kernel (usage still to be found), but encrypted.

You can use the [aio_iob_dumper](tools/aio_iob_dumper) tool to dump and decrypt the firmware files from the DLL. The [README of the tool](tools/aio_iob_dumper/README.md) contains more details on how to use it.

## Protocols

### Stock firmware protocol

Not reversed yet.

### BIO2 protocol

The main firmware of the board implements a Host/Device protocol over the USB serial connection.

The details of this protocol can be found in [lib/protocol/README.md](lib/protocol/README.md).

## Flashing the board

### With MOT file and official Renesas flashing tool

Renesas MCU (on a BIO2 board) can be flashed using their official flashing tool, such as [Renesas Flash Programmer](https://www.renesas.com/en/software-tool/renesas-flash-programmer-programming-gui) or [Renesas Flash Development Toolkit](https://www.renesas.com/en/software-tool/flash-development-toolkit-programming-gui).

Be careful, using these tools will erase the whole flash, so be sure to have a valid MOT file before using them, otherwise you may brick your board.

Requirements:
- A MOT file containing the firmware to flash. You can generate this file using the [generate_mot](tools/generate_mot) tool.
- An official Renesas flashing tool, such as [Renesas Flash Programmer](https://www.renesas.com/en/software-tool/renesas-flash-programmer-programming-gui) (recommanded) or [Renesas Flash Development Toolkit](https://www.renesas.com/en/software-tool/flash-development-toolkit-programming-gui).
- A USB cable to connect the board to your computer.

#### Using Renesas Flash Programmer (recommanded)

Steps to flash the board using Renesas Flash Programmer:
1. Unplug the board and wait for the leds to stop lighting up (if they were on).
2. Short the 2 pins near the USB port to put the board in boot mode (labeled J2 on the PCB).
3. Plug the board while keeping the pins shorted. The board is not anymore detected as COM port, but directly as a USB device.
4. Start Renesas Flash Programmer, select the direct USB connection of the board. If it doesn't appear, try using the other tool or check your USB connection. Please also check your devices manager to see if the board is detected.
5. Select RX62x as the target device
6. Try connecting the board.
7. Enter 12Mhz as the clock frequency.
8. Load the MOT file.
9. Flash the board.
10. Unplug the board, wait for the leds to stop lighting up (if they were on), unshort the pins and plug it again to start it in normal mode.
11. Verify that the board is working correctly using the [bio2_ident](tools/bio2_ident) tool or by verifying the board's PID and VID in the devices manager (should be `0x8040` for BI2A and `0x804C` for BI2X).

Some boards can be picky and return an error code like 0x80. If this happens, you have to unplug, wait for it to go off, plug the board everytime you do a new command. In that case, keep the software open.

If you're on Linux, you can use the following command directly after step 3: `./rfp-cli -osc 12.0 -device RX62N -tool usb -a file.mot`.

#### Using Renesas Flash Development Toolkit

Steps to flash the board using Renesas Flash Development Toolkit:
1. Unplug the board.
2. Short the 2 pins near the USB port to put the board in boot mode (labeled J2 on the PCB).
3. Plug the board while keeping the pins shorted. The board is not anymore detected as COM port, but directly as a USB device.
4. Start the flashing tool, select `Generic Boot Device` if you use `Renesas Flash Development Toolkit`, then select the direct USB connection of the board. If it doesn't appear, try using the other tool or check your USB connection. Please also check your devices manager to see if the board is detected.
5. Enter 12Mhz as the clock frequency
6. Load the MOT file.
7. Flash the board.
8. Unplug the board, wait for the leds to stop lighting up (if they were on), unshort the pins and plug it again to start it in normal mode.
9. Verify that the board is working correctly using the [bio2_ident](tools/bio2_ident) tool or by verifying the board's PID and VID in the devices manager (should be `0x8040` for BI2A and `0x804C` for BI2X).

### With `bio2updatew7.exe`

This executable can be found in the `D:\PCB\` directory of an official arcade cabinet hard drive, and is used to flash the firmware to the board.

It is run when the cabinet boots up in order to flash the firmware to the board and can only be used on a stock BIO2 board with the stock firmware. The executable just ends silently if the board is already flashed with a valid firmware or is broken.

It can be simply be used like this: `bio2updatew7.exe biovideo.bin`.

There are also some options that be used:
- `-debug`: Enable debug output.
- `-debug2`: Enable more debug output.
- `-id nnn`: Target ID number.
- `-skip n`: Skip count

To verify that the board is correctly flashed, you can use the [bio2_ident](tools/bio2_ident) tool.

### Using the internal bi2a firmware updater

The bi2a firmware contains an internal firmware updater that can be used to flash a new firmware to the board. directly over the USB serial connection.

Please take a look at the [bi2a_flash](tools/bi2a_flash) tool for more details on how to use this updater.

This tool can only be used if the board is already flashed with a valid firmware, as it musts be able to handle the flashing protocol.

# Useful links

- [Renesas RX621 manual](https://www.renesas.com/en/document/mah/rx62n-group-rx621-group-users-manual-hardware-rev140)
- [rx-proc-ghidra](https://github.com/redballoonsecurity/rx-proc-ghidra)