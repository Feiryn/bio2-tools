# Bio2 Flash Dumper

This tool is used to dump the flash content of the BIO2 board.

1. Take a valid BI2A firmware file (I used the one from 2018-05-10).
2. Change the hour at offset `0x0020` to a different value (because the firmware checks the date + hour to prevent flashing the same firmware twice).
3. Change the bytes at `0x0664` to `0x00 0xEB` (change ident command address).
4. Change the bytes at `0xEB 0x00` to `EC 13 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 03 7E A5 60 40 FB 42 4C 18 00 00 58 45 F8 05 20 78 75 5B 51 FB 52 4D 18 00 00 58 54 FB 52 4E 18 00 00 58 53 FB 52 4F 18 00 00 58 52 39 8C 8C 62 80 66 01 02` (this is a function that returns 0x20 bytes at the address specified in the command payload).
5. Use the [bio2_flash](../bio2_flash) tool to flash the modified firmware to the board.
6. Use the [bio2_flash_dumper](../bio2_flash_dumper) tool to dump the flash content of the board. Modify the code of this tool to change the address range to dump.

Note that the flash tool will then fail because the ident response is not correct. You can change the flash tool code to bypass the ident check, or use a MOT file to flash the board without using the internal updater.