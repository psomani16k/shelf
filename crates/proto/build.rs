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
        "apipb/config.proto",
        "apipb/auth.proto",
        "serverpb/data.proto",
    ];

    let descriptors = protox::compile(files, [PROTO_ROOT])?;
    // Sorted maps, so the server rewrites `data.json` in a stable, readable order.
    prost_build::Config::new()
        .btree_map([".serverpb"])
        .compile_fds(descriptors.clone())?;

    pbjson_build::Builder::new()
        .register_descriptors(&descriptors.encode_to_vec())?
        .preserve_proto_field_names()
        .ignore_unknown_fields()
        .btree_map([".serverpb"])
        .build(&[".apipb", ".bookkeeperpb", ".configpb", ".statepb", ".serverpb"])?;
    Ok(())
}
