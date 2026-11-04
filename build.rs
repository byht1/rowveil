//! Embeds the Rowveil icon and version info into the Windows executables.

fn main() {
    println!("cargo:rerun-if-changed=assets/rowveil.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/rowveil.ico")
                .set("ProductName", "Rowveil")
                .set("FileDescription", "Rowveil — database explorer and query tool")
                .set("CompanyName", "Rowveil")
                .set("LegalCopyright", "Rowveil");
            if let Err(e) = res.compile() {
                // A missing Windows SDK only costs the icon, not the build.
                println!("cargo:warning=icon not embedded: {e}");
            }
        }
    }
}
