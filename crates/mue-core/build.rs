fn main() {
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/ffmpeg.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let pin: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).expect("Missing FFmpeg release pin"))
            .expect("Invalid FFmpeg release pin");
    let version = pin["version"].as_str().expect("Missing FFmpeg version");
    assert!(
        version.chars().all(|c| c.is_ascii_digit() || c == '.'),
        "Invalid FFmpeg version"
    );
    println!("cargo:rustc-env=MUE_FFMPEG_VERSION={version}");
}
