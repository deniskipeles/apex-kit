fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Set protoc path for portable compilation
    unsafe {
        std::env::set_var("PROTOC", protobuf_src::protoc());
    }

    // 2. Use the correct tonic-prost-build API
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["../proto/replication.proto"], &["../proto"])?;

    Ok(())
}
