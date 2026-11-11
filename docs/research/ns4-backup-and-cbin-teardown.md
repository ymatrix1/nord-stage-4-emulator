# Research: Nord Stage 4 Factory Backup (`.ns4b`) & `CBIN` Format Teardown

- **Checked date:** 2026-11-10
- **Sources:** `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (3,143,490,731 bytes), `Nord Stage 4 User Manual v1.6X-Edition-N.pdf`
- **Question:** How are Nord Stage 4 programs, engine presets, and backups structured at the byte level, and can we parse and round-trip them losslessly?

---

## 1. Archive & Manifest Findings

1. **PKZIP Container:** `Nord Stage 4 Factory Restore RevK v1.66.ns4b` starts with standard ZIP magic `50 4B 03 04 14 00`.
2. **Root `meta.xml`:**
   ```xml
   <?xml version="1.0" encoding="UTF-8"?>
   <backup file_version="1" backup_format_version="1" product_id="46" product_version="166" product_build="2934" product_content_version="0" manager_version="918" manager_build="1444"/>
   ```
   - `product_id="46"` (`0x2E`) uniquely identifies the Nord Stage 4 hardware family.
   - `product_version="166"` and `product_build="2934"` match OS v1.66.
3. **Complete File Census (1,171 total files):**
   - `ns4p` (Program): 384 files (Banks A–H × 48 programs), fixed size **868 bytes**
   - `ns4l` (Live): 8 files (`Live 0.ns4l`..`Live 7.ns4l`), fixed size **868 bytes**
   - `ns4y` (Synth Preset): 384 files, fixed size **541 bytes**
   - `ns4n` (Piano Preset): 96 files, fixed size **195 bytes**
   - `ns4o` (Organ Preset): 64 files, fixed size **183 bytes**
   - `ns4t` (System Settings): 1 file (`Settings/Bank 1/Settings.ns4t`), fixed size **124 bytes**
   - `npno` (Nord Piano Library): 38 files
   - `nsmp4` (Nord Sample Library 4): 195 files
   - `xml`: 1 file (`meta.xml`)

---

## 2. `CBIN` v1 48-Byte Header Discovery

All preset files share an identical 48-byte (`0x30`) header layout:

```text
Offset  Hex (12dB Sweep.ns4p)                            Meaning
0x0000  43 42 49 4e                                      Magic "CBIN"
0x0004  01 00 00 00                                      Container version = 1 (u32 LE)
0x0008  6e 73 34 70                                      Type tag "ns4p"
0x000c  00 00                                            Bank index = 0 (Bank A)
0x000e  26 00                                            Program index = 0x26 (38)
0x0010  2d 00 00 00                                      Category ID = 0x2d (Synth)
0x0014  39 01 00 00                                      Schema ID LE = 0x0139 (313)
0x0018  04 1e fc cf                                      Payload CRC32 / checksum
0x001c  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  18-byte reserved block (zeros)
0x002c  00 00                                            (end of 18-byte reserved block)
0x002e  01 39                                            Schema ID BE = 0x0139 (313)
0x0030  2c a1 37 10 2b 44 ...                            Start of bit-packed payload (820 bytes)
```

---

## 3. Sub-Preset Bitstream Embedding

Comparing `Organ Preset/Bank 1/B3 Basic.ns4o` (183 bytes) with `Live/Bank 1/Live 1.ns4l` (868 bytes):
- In `B3 Basic.ns4o` at offset `0x30`:
  `11 27 fb fb fb ff f7 f7 f7 f5 ad b0 00 00 00 05 6d 80 03 50 08 00 17 84 21 10 84 22 10 84 42 10 80 42 10`
- In `Live 1.ns4l` (and factory `.ns4p` programs) at offset `0x5D`:
  `11 27 fb fb fb ff f7 f7 f7 f5 ad b0 00 00 00 05 6d 80 03 50 08 00 17 84 21 10 84 22 10 84 42 10 80 42 10`

Furthermore, comparing `B3 Basic.ns4o` (`88 8000 000` drawbars) vs `B3 76 Solo.ns4o`:
- `B3 Basic.ns4o` @ `0x3A`: `ad b0 00 00 00 05 6d 80 03 50 08`
- `B3 76 Solo.ns4o` @ `0x3A`: `ed b9 60 00 00 15 6d 80 03 90 08`

This proves that `.ns4p` is a composite container whose 820-byte payload concatenates:
- `45` bytes of Global Program / Split / Scene / Morph header (`0x30..0x5C`),
- `135` bytes of Organ Preset bitstream (`0x5D..0xE3`),
- `147` bytes of Piano Preset bitstream (`0xE4..0x176`),
- `493` bytes of Synth Preset bitstream (`0x177..0x363`).
Total: $48 + 45 + 135 + 147 + 493 = 868\text{ bytes}$!

---

## 4. Implications & Follow-ups

- **Implication 1:** Any parser for `.ns4o` (135-byte payload), `.ns4n` (147-byte payload), and `.ns4y` (493-byte payload) automatically decodes the corresponding sections inside `.ns4p` and `.ns4l` at offsets `0x5D`, `0xE4`, and `0x177`!
- **Follow-up:** Map every bit offset inside the 135-byte Organ, 147-byte Piano, and 493-byte Synth payloads via differential bit analysis (`tools/ns4_inspect.py`).
