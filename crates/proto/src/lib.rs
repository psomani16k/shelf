pub mod bookkeeping {
    include!(concat!(env!("OUT_DIR"), "/bookkeeperpb.rs"));
    include!(concat!(env!("OUT_DIR"), "/bookkeeperpb.serde.rs"));
}
