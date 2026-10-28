fn main() {
    println!("cargo:rerun-if-env-changed=PIMP_UPDATER_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=PIMP_BUILD_COMMIT");
    tauri_build::build()
}
