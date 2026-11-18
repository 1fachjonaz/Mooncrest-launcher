fn main() {
    // The launcher files that get installed, packed by tools/build-setup.sh.
    let payload = std::path::Path::new("../payload/payload.zip");
    println!("cargo:rerun-if-changed=../payload/payload.zip");
    if !payload.exists() {
        panic!("setup/payload/payload.zip is missing – run tools/build-setup.sh");
    }
    tauri_build::build()
}
