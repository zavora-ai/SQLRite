fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/sqlrite/v1/query_service.proto");
    tonic_build::configure()
        .compile_protos(&["proto/sqlrite/v1/query_service.proto"], &["proto"])?;
    Ok(())
}
