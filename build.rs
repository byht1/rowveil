//! Embeds the QueryNest icon and version info into the Windows executables.

fn main() {
    println!("cargo:rerun-if-changed=assets/querynest.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/querynest.ico")
                .set("ProductName", "QueryNest")
                .set("FileDescription", "QueryNest — database explorer and query tool")
                .set("CompanyName", "QueryNest")
                .set("LegalCopyright", "QueryNest");
            if let Err(e) = res.compile() {
                // A missing Windows SDK only costs the icon, not the build.
                println!("cargo:warning=icon not embedded: {e}");
            }
        }
    }
}
