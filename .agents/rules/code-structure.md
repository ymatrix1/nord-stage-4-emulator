# Code structure

- **Small, single-purpose files.** Keep a source file under about 400 lines (code plus its tests). When a file grows past that, split it into a directory module (`foo/mod.rs` plus focused submodules) before adding more.
- **One concern per module**: state and shared helpers in `mod.rs`, each subsystem (memory, compute unit, decoder stage, ...) in its own file. Re-export the public API from `mod.rs` so callers do not change.
- **DRY.** Extract a helper the second time a pattern repeats (field extraction, register-bank offsets, "run compute or skip", memory transfers). Share one implementation between the long and short forms of an operation instead of keeping two copies.
- **Every submodule keeps its own unit tests** (`tools/check_tests.py` enforces this). Shared test fixtures go in a `#[cfg(test)] mod support;` file that has its own test.
- **Refactors preserve behaviour**: show identical output on real inputs (for example a byte-identical disassembly listing) and a before/after benchmark when the code is on a hot path.
