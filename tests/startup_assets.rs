use std::{fs, process::Command};
use topcoat::asset::AssetBundle;

#[test]
fn normal_startup_installs_the_embedded_asset_bundle() {
    let test_dir =
        std::env::temp_dir().join(format!("rtmp-proxy-startup-assets-{}", std::process::id()));
    let executable = test_dir.join("rtmp-proxy");
    let missing_config = test_dir.join("missing-config.json");

    fs::create_dir_all(&test_dir).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_rtmp-proxy"), &executable).unwrap();

    let status = Command::new(&executable)
        .arg("--config")
        .arg(&missing_config)
        .status()
        .unwrap();
    assert!(
        !status.success(),
        "startup unexpectedly accepted a missing config"
    );

    let bundle = AssetBundle::load_dir(test_dir.join("assets")).unwrap();
    assert_eq!(bundle.catalog().assets().count(), 6);

    fs::remove_dir_all(test_dir).unwrap();
}
