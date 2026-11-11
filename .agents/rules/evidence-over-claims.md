# Evidence over claims

- Don't say a change works until a test or check that exercises it ran **after** your last edit and passed. Show the command and relevant output.
- If you couldn't run a check, say so and why.
- Binary format claims (byte offsets, header fields, schema IDs, sizes) must cite either a deterministic unit test or empirical output from `tools/ns4_inspect.py`.
- DSP stability and frequency-response claims must be backed by deterministic unit tests in `crates/nord-core/src/`.
