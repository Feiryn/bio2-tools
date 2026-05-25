# BI2X Protocol

The BI2X protocol is the communication protocol used between a host computer and a BI2X board. It differs significantly from the BI2A protocol. This crate implements frame encoding/decoding, command serialization, and response parsing.

## Initialization Sequence

The board is typically brought up with the following sequence:

1. Send command `0x01` to node `0x00` to initialize the board and enable node `0x02`.
2. Send command `0x02` to node `0x02` to retrieve firmware information and verify the board state.
3. Send command `0x10` to node `0x02` to allocate memory for the game firmware; record the returned handle.
4. Send command `0x13` to node `0x02` repeatedly to upload the game firmware in chunks (≤ 64 bytes each), using the handle and the current byte offset.
5. Send command `0x78` to node `0x02` to finalize and boot the uploaded firmware; record the returned firmware handle for subsequent game-specific commands.

## Frame Format

| Offset | Size | Description |
|--------|------|-------------|
| 0 | 1 | Start byte (`0xAA`) |
| 1 | 1 | Destination node |
| 2 | 1 | Sequence number |
| 3 | 1–4 | Payload length, VLQ-encoded |
| 3+*L* | 1 | Flags byte (mode, encryption, header CRC) |
| 4+*L* | *length* | Encoded/encrypted payload |
| 4+*L*+*length* | 1 | Payload CRC |

*L* = number of VLQ bytes used to encode the length field.

### Start Byte

Always `0xAA`. Leading `0xAA` bytes may appear before the frame and must be ignored by the receiver.

### Destination Node

| Value | Description |
|-------|-------------|
| `0x00` | Board initialization only (command `0x01`) |
| `0x01` | Not reversed yet; |
| `0x02` | All post-initialization commands |
| `0x03` | Responses to commands sent to node `0x02` |

### Sequence Number

An 8-bit counter incremented by the host for each command. Used to correlate responses with their requests.

The values `0x00`, `0xAA`, and `0xFF` are reserved and must not be used as sequence numbers.

### Payload Length (VLQ)

The length is encoded as a variable-length quantity. Byte classification:

| Byte range | Role | Data bits |
|------------|------|-----------|
| `0x00`–`0x7F` | Terminal | Bits 6–0 (7 bits) |
| `0xC0`–`0xFF` | Continuation | Bits 5–0 (6 bits) |
| `0x80`–`0xBF` | Invalid | — |

Length bits are accumulated left-to-right from each continuation byte, followed by the terminal byte. The encoded size depends on the value:

| Value range | Bytes |
|-------------|-------|
| 0–127 | 1 |
| 128–16 383 | 2 |
| 16 384–2 097 151 | 3 |
| 2 097 152–268 435 455 | 4 |

See `encode_length` / `decode_length` in [src/frame.rs](src/frame.rs).

### Flags Byte

This byte may itself be escaped: if it equals `0xFF`, read the following byte and bit-invert it to obtain the actual flags value.

| Bits | Field | Description |
|------|-------|-------------|
| 7–5 | Mode | Payload encoding mode (see table below) |
| 4 | Encrypted | `1` if the payload is encrypted |
| 3–0 | Header CRC | CRC4-LGP of the header, stored XOR'd with `0x0F` |

#### Payload Modes (Bits 7–5)

| Bits 7–5 | Mode | Description |
|-----------|------|-------------|
| `000` | Escaped | `0xFF` escapes the next byte by bit-inverting it |
| `010` | Raw | Payload is transmitted as-is |
| `011` | Byte substitution | First payload byte is a substitution marker; all occurrences of that byte in the remainder are replaced with `0xAA` |
| `100` | MC_LZ | Payload is compressed with MC_LZ (see [../compression/src/mc_lz.rs](../compression/src/mc_lz.rs)) |

All other mode combinations are reserved.

#### Encryption

When bit 4 is set, encryption is applied to the payload **before** any encoding mode is applied. The sequence number is used as the seed. See `encrypt_payload` / `decrypt_payload` in [../crypt/src/bi2x_payload.rs](../crypt/src/bi2x_payload.rs).

#### Header CRC

CRC4-LGP computed over: destination node, sequence number, all raw VLQ bytes, and the upper nibble of the flags byte. The result is XOR'd with `0x0F` and stored in bits 3–0 of the flags byte. See `get_header_crc` in [src/frame.rs](src/frame.rs).

**Example:** flags byte `0x92` (`1001 0010`):
- Bits 7–5 = `100` → MC_LZ compressed
- Bit 4 = `1` → encrypted
- Bits 3–0 = `0x2` → stored CRC nibble (actual CRC = `0x?2 ^ 0x0F = 0x02`)

### Payload Data

The decoded payload contains one or more records concatenated back-to-back:

| Offset | Size | Description |
|--------|------|-------------|
| 0 | 1 | Destination handle |
| 1 | 1 | Command byte |
| 2+ | variable | Command-specific data |

**Destination handle:** `0x00` for the built-in commands below; the firmware handle returned by command `0x78` for game-specific commands. Multiple records can be batched in a single frame as long as they all target the same node.

### Payload CRC

CRC7-LGP48 computed over the fully decoded and decrypted payload bytes. See `payload_crc` in [src/frame.rs](src/frame.rs).

## Commands

### Node `0x00`

#### `0x01` — Initialize Board

Initializes the board and enables commands on node `0x02`.

**Request**

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x01` |
| Data | — |

**Response** (node `0x01`)

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x01` |
| Status | `0x00` = success; non-zero = already initialized or error |

---

### Node `0x02`

#### `0x02` — Get Firmware Information

Retrieves 33 bytes of raw info from the BI2X kernel firmware.

**Request**

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x02` |
| Data | `0x81` |

**Response** (node `0x03`)

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x02` |
| Data | 33 bytes of raw firmware information |

---

#### `0x10` — Allocate Memory

Allocates a contiguous buffer for a game firmware upload. Returns a handle used in commands `0x13` and `0x78`.

**Request**

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x10` |
| Data | 4-byte allocation size, big-endian |

**Response** (node `0x03`)

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x10` |
| Status | `0x00` = success; non-zero = error |
| Alloc handle | 1 byte; memory handle to use in subsequent commands |

---

#### `0x13` — Upload Firmware Chunk

Writes one chunk of the game firmware into the allocated buffer. Chunks must be 64 bytes or fewer.

**Request**

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x13` |
| Data | 1-byte alloc handle + 4-byte offset (big-endian) + chunk bytes (≤ 64) |

**Response** (node `0x03`)

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x13` |
| Status | `0x00` = success; non-zero = error |

---

#### `0x78` — Finalize and Initialize Firmware

Finalizes the upload and boots the game firmware. Returns the handle used for all subsequent game-specific commands.

**Request**

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x78` |
| Data | 1-byte alloc handle + 4-byte firmware descriptor offset (big-endian, typically `0x00000000`) + init arguments |

**Response** (node `0x03`)

| Field | Value |
|-------|-------|
| Handle | `0x00` |
| Command | `0x78` |
| Status | `0x00` = success; non-zero = error |
| Firmware handle | 1 byte; handle for subsequent game-specific commands |

---

#### Game-Specific Commands

After the firmware is initialized, send commands using the firmware handle returned by `0x78`. The command byte and data format are firmware-specific and must be reverse-engineered per game.

**Request**

| Field | Value |
|-------|-------|
| Handle | Firmware handle (from `0x78`) |
| Command | Game-specific |
| Data | Game-specific |
