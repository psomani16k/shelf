use prost::Message;

const PROTO_ROOT: &str = "../../proto";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={PROTO_ROOT}");
    let files = [
        "apipb/list.proto",
        "bookkeeperpb/metadata.proto",
        "bookkeeperpb/progress.proto",
        "configpb/config.proto",
        "statepb/state.proto",
        "apipb/error.proto",
        "apipb/progress.proto",
        "apipb/state.proto",
        "apipb/library.proto",
    ];

    let descriptors = protox::compile(files, [PROTO_ROOT])?;
    prost_build::Config::new().compile_fds(descriptors.clone())?;

    pbjson_build::Builder::new()
        .register_descriptors(&descriptors.encode_to_vec())?
        .preserve_proto_field_names()
        .ignore_unknown_fields()
        .build(&[".apipb", ".bookkeeperpb", ".configpb", ".statepb"])?;
    Ok(())
}
