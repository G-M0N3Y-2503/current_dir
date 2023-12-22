fn main() {
    std::env::var("RUSTUP_TOOLCHAIN")
        .unwrap()
        .starts_with("nightly")
        .then(|| println!("cargo:rustc-cfg=feature=\"nightly\""));
}
