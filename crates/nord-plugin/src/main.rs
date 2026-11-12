use nih_plug::prelude::*;
use nord_plugin::NordStage4Plugin;

fn main() {
    nih_export_standalone::<NordStage4Plugin>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standalone_target_symbols() {
        assert_eq!(NordStage4Plugin::NAME, "Nord Stage 4");
    }
}
