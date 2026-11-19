#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icon.ico");

    // Embed the application icon as resource ID 1 in the Windows executable. GPUI loads the
    // window-class icon from that resource (`LoadImageW(module, 1, IMAGE_ICON, ..)`), so
    // without it the taskbar, title bar and Explorer fall back to the default icon.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon resource: {e}");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let mut search_dirs: Vec<PathBuf> = Vec::new();
        if let Ok(arch) = env::var("CARGO_CFG_TARGET_ARCH") {
            if !arch.is_empty() {
                search_dirs.push(PathBuf::from(format!("/usr/lib/{arch}-linux-gnu")));
            }
        }
        for dir in [
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/usr/lib64",
            "/usr/lib",
        ] {
            let p = PathBuf::from(dir);
            if !search_dirs.contains(&p) {
                search_dirs.push(p);
            }
        }

        // Check if libxkbcommon-x11.so exists in standard search paths
        let has_xkb_dev = search_dirs
            .iter()
            .any(|dir| dir.join("libxkbcommon-x11.so").exists());

        if !has_xkb_dev {
            // Fall back to creating a symlink to libxkbcommon-x11.so.0 in OUT_DIR
            if let Some(candidate_so) = search_dirs
                .iter()
                .map(|dir| dir.join("libxkbcommon-x11.so.0"))
                .find(|p| p.exists())
            {
                let symlink_target = out_dir.join("libxkbcommon-x11.so");
                if symlink_target.symlink_metadata().is_ok() && !symlink_target.exists() {
                    let _ = std::fs::remove_file(&symlink_target);
                }
                if !symlink_target.exists() {
                    let _ = std::os::unix::fs::symlink(&candidate_so, &symlink_target);
                }
                println!("cargo:rustc-link-search=native={}", out_dir.display());
            }
        }
    }
}
