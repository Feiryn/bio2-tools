# Biovideo Encryption/Decryption Tool

This tool can either decrypt a biovideo file into its bio2base and firmware components, or encrypt a bio2base and firmware into a biovideo file.

The biovideo file is used for BI2A only.

## Usage

To decrypt a biovideo file:

```
biovideo_crypt decrypt <input_biovideo_file> <output_bio2base_file> <output_firmware_file>
```

To encrypt a bio2base and firmware into a biovideo file:

```
biovideo_crypt encrypt <input_bio2base_file> <input_firmware_file> <output_biovideo_file>
```

## Encryption/Decryption details

The encryption is using a slightly modified version of AES-128 in ECB mode. The full algorithm is in the code of the crypt lib.

Decrypting using AES then gives us the bio2base file. The firmware is encrypted in the bio2base file using a XOR layer at offset 0x10000.