//! Windows only: gives the .exe its file icon and its version information, so Explorer, the task
//! bar entry and the Properties dialog show NoteCraft. The version is the workspace version.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/notecraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/notecraft.ico");
    res.set("ProductName", "NoteCraft");
    res.set("FileDescription", "NoteCraft (alpha)");
    res.set("LegalCopyright", "MIT OR Apache-2.0");
    if let Err(e) = res.compile() {
        // Without the resource the program still works, only with the default file icon.
        println!("cargo:warning=could not embed the icon: {e}");
    }
}
