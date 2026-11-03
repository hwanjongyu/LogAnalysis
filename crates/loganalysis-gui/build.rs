use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Check if libxkbcommon-x11.so exists in standard search paths
    let has_xkb_dev = Path::new("/usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so").exists()
        || Path::new("/usr/lib/libxkbcommon-x11.so").exists();

    if !has_xkb_dev {
        // Fall back to creating a symlink to libxkbcommon-x11.so.0 in OUT_DIR
        let candidate_so = Path::new("/usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0");
        if candidate_so.exists() {
            let symlink_target = out_dir.join("libxkbcommon-x11.so");
            if !symlink_target.exists() {
                let _ = std::os::unix::fs::symlink(candidate_so, &symlink_target);
            }
            println!("cargo:rustc-link-search=native={}", out_dir.display());
        }
    }
}
