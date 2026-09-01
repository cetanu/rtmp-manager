fn main() {
    println!("cargo:rerun-if-changed=migrations");
    topcoat::tailwind::BuildConfig::new()
        .input("styles.css")
        .render()
        .unwrap();
}
