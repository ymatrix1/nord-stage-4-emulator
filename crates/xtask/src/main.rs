//! Workspace bundler helper for `.clap` and `.vst3` plugin targets.

fn main() -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::main()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_xtask_bundle_helper_exists() {
        let _main_fn: fn() -> nih_plug_xtask::Result<()> = super::main;
    }
}
