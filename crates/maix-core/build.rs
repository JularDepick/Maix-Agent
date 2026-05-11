use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Project root is ../../ from crates/maix-core
    let proto_dir: PathBuf = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("..")
        .join("..")
        .join("proto");

    let proto_files = [
        proto_dir.join("common.proto"),
        proto_dir.join("maix.proto"),
        proto_dir.join("session.proto"),
        proto_dir.join("tool.proto"),
        proto_dir.join("memory.proto"),
        proto_dir.join("task.proto"),
        proto_dir.join("skill.proto"),
        proto_dir.join("agent.proto"),
    ];

    // Ensure all proto files exist
    for f in &proto_files {
        if f.exists() {
            println!("cargo:rerun-if-changed={}", f.display());
        } else {
            eprintln!("warning: proto file not found: {}", f.display());
        }
    }

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&proto_files, std::slice::from_ref(&proto_dir))
        .unwrap_or_else(|e| panic!("Failed to compile proto files: {e}"));

    // Include google.protobuf well-known types
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
