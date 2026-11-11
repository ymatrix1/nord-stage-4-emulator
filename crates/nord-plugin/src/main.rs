//! Standalone desktop binary entry point for the `nord-plugin` wrapper.

fn main() {
    nih_plug::nih_export_standalone::<nord_plugin::NordStage4Plugin>();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_standalone_plugin_constructs() {
        let _plugin = nord_plugin::NordStage4Plugin::default();
    }
}
