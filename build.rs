fn main() {
    cc::Build::new()
        .file("src/ffi/c_src/entropy.c")
        .opt_level(2)
        .compile("entropy");

    // Ensure the static library is linked into both lib and bin targets.
    println!("cargo:rustc-link-lib=static=entropy");
    println!("cargo:rerun-if-changed=src/ffi/c_src/entropy.c");
}
