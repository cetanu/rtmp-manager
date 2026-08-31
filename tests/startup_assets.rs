use std::{fs, process::Command, thread, time::Duration};
use topcoat::asset::AssetBundle;

#[test]
fn normal_startup_installs_the_embedded_asset_bundle() {
    let test_dir =
        std::env::temp_dir().join(format!("rtmp-proxy-startup-assets-{}", std::process::id()));
    let executable = test_dir.join("rtmp-proxy");
    let database = test_dir.join("rtmp-manager.sqlite3");

    fs::create_dir_all(&test_dir).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_rtmp-proxy"), &executable).unwrap();

    let mut child = Command::new(&executable)
        .env(
            "DATABASE_URL",
            format!("sqlite://{}?mode=rwc", database.display()),
        )
        .spawn()
        .unwrap();

    for _ in 0..50 {
        if database.exists() && test_dir.join("assets").exists() {
            break;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "server exited during first-run startup"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        database.exists(),
        "first-run startup did not create its database"
    );
    child.kill().unwrap();
    child.wait().unwrap();

    let bundle = AssetBundle::load_dir(test_dir.join("assets")).unwrap();
    assert_eq!(bundle.catalog().assets().count(), 11);

    fs::remove_dir_all(test_dir).unwrap();
}
