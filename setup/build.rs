//! Packs the app files into a compressed payload that is embedded in the setup executable.
//!
//! GLYPHLING_PAYLOAD_DIR points to the folder with the built app (glyphling.exe and, for
//! windows-gnu builds, WebView2Loader.dll). Without it the setup is built with an empty
//! payload, which is enough to compile and check the code.

use std::io::Write;

fn main() {
    tauri_build::build();

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.bin");
    let mut buf: Vec<u8> = Vec::new();
    println!("cargo:rerun-if-env-changed=GLYPHLING_PAYLOAD_DIR");
    if let Ok(dir) = std::env::var("GLYPHLING_PAYLOAD_DIR") {
        for name in ["glyphling.exe", "WebView2Loader.dll"] {
            let path = std::path::Path::new(&dir).join(name);
            println!("cargo:rerun-if-changed={}", path.display());
            let Ok(data) = std::fs::read(&path) else { continue };
            let packed = miniz_oxide::deflate::compress_to_vec(&data, 9);
            buf.write_all(&(name.len() as u16).to_le_bytes()).unwrap();
            buf.write_all(name.as_bytes()).unwrap();
            buf.write_all(&(data.len() as u64).to_le_bytes()).unwrap();
            buf.write_all(&(packed.len() as u64).to_le_bytes()).unwrap();
            buf.write_all(&packed).unwrap();
        }
    }
    std::fs::write(&out, buf).unwrap();

    // the version shown by the setup is the app's, read from its tauri.conf.json
    let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("../src-tauri/tauri.conf.json").unwrap()).unwrap();
    println!("cargo:rustc-env=GLYPHLING_VERSION={}", conf["version"].as_str().unwrap());
    println!("cargo:rerun-if-changed=../src-tauri/tauri.conf.json");
}
