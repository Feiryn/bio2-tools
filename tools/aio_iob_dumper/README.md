# libaio-iob.dll dumper

This tool is used to dump and decrypt the firmware files contained in the `libaio_iob.dll` file.

It outputs the following decrypted firmware files:
- `bio2wrfirm.bin`: The bio2wrfirm firmware
- `bi2x_firmware.bin`: The bi2x firmware
- `bi2x_kernel.bin`: The bi2x kernel (usage still to be found)

This tool has been tested with multiple versions of the libaio_iob.dll, but it may not work with future versions if the firmware files are moved or encrypted differently. If you encounter any issues with the tool, please report them to the developer.

## Usage

```
./aio_iob_dumper <path_to_libaio_iob.dll> <output_directory>
```