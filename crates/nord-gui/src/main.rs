//! Nord Stage 4 Standalone Application Entry Point.

fn main() {
    println!("Nord Stage 4 Emulator");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_main_version() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
