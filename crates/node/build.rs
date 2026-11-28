// Node-API symbols are resolved by the Node process that loads the module: napi-build sets the linker flags for that
// (on macOS `-undefined dynamic_lookup`; on Windows napi-rs loads the symbols from the host at run time).
fn main() {
    napi_build::setup();
}
