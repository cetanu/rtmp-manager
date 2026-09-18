use std::{fs, process::Command};
use topcoat::asset::AssetBundle;

#[test]
fn topcoat_bundle_is_loadable_by_startup() {
    let test_dir =
        std::env::temp_dir().join(format!("rtmp-proxy-startup-assets-{}", std::process::id()));
    let executable = test_dir.join("rtmp-proxy");
    let missing_config = test_dir.join("missing-config.json");

    fs::create_dir_all(&test_dir).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_rtmp-proxy"), &executable).unwrap();

    let status = Command::new("topcoat")
        .args(["asset", "bundle", "--bin", "rtmp-proxy"])
        .status()
        .expect("topcoat CLI should be installed");
    assert!(status.success(), "topcoat asset bundle failed");
    fs::create_dir_all(test_dir.join("assets")).unwrap();
    for entry in fs::read_dir("target/debug/assets").unwrap() {
        let entry = entry.unwrap();
        fs::copy(
            entry.path(),
            test_dir.join("assets").join(entry.file_name()),
        )
        .unwrap();
    }

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
    assert!(
        bundle.catalog().assets().next().is_some(),
        "asset bundle should not be empty"
    );
    assert!(
        bundle.get(topcoat::runtime::SCRIPT.id()).is_some(),
        "bundle should include topcoat runtime script"
    );

    fs::remove_dir_all(test_dir).unwrap();
}
