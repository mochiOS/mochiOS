fn main() {
    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(value) => value,
        Err(error) => {
            eprintln!("CARGO_MANIFEST_DIR not set: {}", error);
            std::process::exit(1);
        }
    };
    println!("cargo:rustc-link-arg=-T{}/kernel.ld", manifest_dir);
    println!("cargo:rerun-if-changed=kernel.ld");
}
