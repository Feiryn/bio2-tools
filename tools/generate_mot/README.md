# Generate MOT

This tool is used to generate a MOT file that can be flashed to the BIO2 board using the official Renesas flashing tool. This is useful to flash a custom firmware without using the internal updater of the board, which requires a working firmware. You can use this method to recover a bricked board, because it doesn't require the board to be functional to flash it.

## Inputs

You'll need the following files to generate a MOT file:
- The data flash avaiable in [assets/data_flash.bin](../assets/data_flash.bin).
- For bi2a:
  - The bio2base file dumped using [biovideo_crypt](../biovideo_crypt).
  - The bi2a firmware file also dumped using [biovideo_crypt](../biovideo_crypt).
- For bi2x:
  - The bio2wrfirm file extracted from the libaio_iob.dll using the [aio_iob_dumper](../aio_iob_dumper) (the bio2wrfirm.bin file).
  - The bi2x firmware file also extracted from the libaio_iob.dll using the [aio_iob_dumper](../aio_iob_dumper) (the bi2x_firmware.bin file). 

## Usage

> [!IMPORTANT]
> 
> Be consistent with both input files.
> 
> bi2a requires a bio2base and a bi2a firmware (both dumped from a biovideo file), and bi2x requires a bio2wrfirm and a bi2x firmware (both extracted from the libaio_iob.dll). Mixing the files will not work.

### For BI2A

```
./generate_mot bi2a data_flash.bin bio2base.bin bi2a_firmware.bin output.mot
```

### For BI2X

```
./generate_mot bi2x data_flash.bin bio2wrfirm.bin bi2x_firmware.bin output.mot
```