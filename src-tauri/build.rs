fn main() {
    println!("cargo:rerun-if-env-changed=PIMP_UPDATER_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=PIMP_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=PIMP_RELEASE_CLASSIFICATION");
    println!("cargo:rerun-if-env-changed=PIMP_RELEASE_DATE");
    tauri_build::build()
}
