fn main() {
    cc::Build::new()
        .file("src/ffi/c_src/entropy.c")
        .opt_level(2)
        .compile("entropy");

    println!("cargo:rerun-if-changed=src/ffi/c_src/entropy.c");
}
