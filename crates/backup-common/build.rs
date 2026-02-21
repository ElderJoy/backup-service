fn main() {
    // --- C FFI: compile entropy.c ---
    cc::Build::new()
        .file("src/ffi/c_src/entropy.c")
        .opt_level(2)
        .compile("entropy");

    println!("cargo:rerun-if-changed=src/ffi/c_src/entropy.c");

    // --- gRPC: compile proto definitions ---
    tonic_build::compile_protos("../../proto/backup.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos: {e}"));
    println!("cargo:rerun-if-changed=../../proto/backup.proto");
}
