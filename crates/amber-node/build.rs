fn main() {
    // Configures the platform linker so the N-API symbols resolve at load time
    // (e.g. `-undefined dynamic_lookup` on macOS).
    napi_build::setup();
}
