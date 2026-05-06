# Generate MOT

This tool is used to generate a MOT file that can be flashed to the BIO2 board using the official Renesas flashing tool. This is useful to flash a custom firmware without using the internal updater of the board, which requires a working firmware. You can use this method to recover a bricked board, because it doesn't require the board to be functional to flash it.

## Inputs

You'll need the following files to generate a MOT file:
- The data flash avaiable in [assets/data_flash.bin](../assets/data_flash.bin).
- The bio2base/bio2wrfirm either dumped using [biovideo_crypt](../biovideo_crypt) or extracted from the libaio_iob.dll using the [aio_iob_dumper](../aio_iob_dumper).
- The firmware you want to flash, also dumped using [biovideo_crypt](../biovideo_crypt) or extracted from the libaio_iob.dll using the [aio_iob_dumper](../aio_iob_dumper).

## Usage

```
generate_mot <data_flash_file> <bio2base_file> <firmware_file> <output_mot_file>
```