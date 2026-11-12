# SHARC fixed-point probes (ADSP-21489 simulator)

`gen_fixed_probe.py`: generator (needs `easm21k`/`elfdump` on PATH). `build_csv.py`: joins the memory dumps with the layout. `fixed_measured.csv`: result, columns `build,mode1,name,short,word,x,y,p0,p1,p2,r0,mr0,mr1,mr2,astat,stky` (hex). Procedure and findings: `../sharc-fixed.md`; the check is `crates/nord-sharc/src/core/fixed_measured.rs`.
