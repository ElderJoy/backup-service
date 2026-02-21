fn main() {
    // --- C FFI: compile entropy.c ---
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = std::path::Path::new(&out_dir);

    cc::Build::new()
        .file("src/ffi/c_src/entropy.c")
        .opt_level(2)
        .compile("entropy");

    // Cargo only links build-script libs into the library target; explicitly link into binaries.
    let lib_name = if std::env::var("TARGET").map_or(false, |t| t.contains("windows")) {
        "entropy.lib"
    } else {
        "libentropy.a"
    };
    println!(
        "cargo:rustc-link-arg-bins={}",
        out_path.join(lib_name).display()
    );
    println!("cargo:rerun-if-changed=src/ffi/c_src/entropy.c");

    // --- gRPC: compile proto definitions ---
    tonic_build::compile_protos("proto/backup.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos: {e}"));
    println!("cargo:rerun-if-changed=proto/backup.proto");
}
