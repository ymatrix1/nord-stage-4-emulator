# SHARC seed measurement (ADSP-21489 simulator)

Files: `seed_probe.asm`, `seed_probe.c` (probe), `seeds_full.csv` (input, RECIPS, RSQRTS words), `seeds_specials_flags.csv` (ASTAT after each special input). Procedure and findings: `../sharc-float.md`. Re-run: build with `cc21k` (see there), load `probe.dxe` on the CCES ADSP-21489 simulator, run, dump the memory block starting at `_flags_rec`.
