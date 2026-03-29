fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/sqlrite/v1/query_service.proto");
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }
    tonic_build::configure()
        .compile_protos(&["proto/sqlrite/v1/query_service.proto"], &["proto"])?;
    Ok(())
}
