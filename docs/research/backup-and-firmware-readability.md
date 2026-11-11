# Research: Backup Contents, Sample Libraries & Firmware Readability Checks

- **Checked date:** 2026-11-10
- **Sources (local, git-ignored):** `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, `Update Nord Stage 4 OS v1.66.exe`
- **Question:** Can samples and DSP algorithms be recovered from what we have?

## 1. `.ns4b` backup is a plain ZIP (1,171 entries, stored uncompressed)

| Type | Files | Size | Content |
|---|---:|---:|---|
| `.ns4p` | 384 | 0.3 MB | Programs |
| `.ns4y` | 384 | 0.2 MB | Synth presets |
| `.ns4n` / `.ns4o` / `.ns4l` / `.ns4t` | 96 / 64 / 8 / 1 | tiny | Piano / Organ presets, Live, Settings |
| `.nsmp4` | 195 | 1,035 MB | Sample Library 4 (synth/sample sounds) |
| `.npno` | 38 | 2,107 MB | Piano library (grands, uprights, electrics, clavs) |
| `meta.xml` | 1 | tiny | Backup manifest |

## 2. Piano and sample files

- Both begin with the standard 48-byte `CBIN` header (tags `npno` / `nsmp`) followed by a `CNSP` payload with the piano name in plain text.
- Payload entropy is about 7.5 bits per byte, so it is compressed or encoded audio. The sample codec is **unknown**; nothing here decodes it yet.
- These are Clavia/Nord licensed content. They must **not** be committed or redistributed. The emulator ships with open SFZ libraries instead (OQ1) and may add a user-supplied import path.

## 3. Firmware readability (`Res 101`, 4,037,080 B)

- The `ARM0` image is **plain ARM code** (instruction words such as `mcr p15` and `msr cpsr` decode directly), so the host side is analyzable.
- Loader-table strings at `0x3CF040` are plain text.
- The `0x080000..0x37FFFF` range has entropy 7.1 to 7.7, which is consistent with compressed data **or** encrypted data. We cannot tell which yet.
- The `DSPO` / `DSPS` / `DSPF` images were **not** located as readable SHARC code.

## 4. Decisions

- No attempt is made to defeat any encryption or protection. If the DSP images turn out to be encrypted, DSP algorithm recovery stops there.
- Allowed next step: test whether the high-entropy region is a standard compression format (zlib / LZ-type headers). If so, decompress and re-examine for tables and constants.
- The synth, organ and effects engines continue on textbook models validated by ear.
