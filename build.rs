fn main() {
    println!("cargo:rerun-if-changed=assets/icons/super-platinum.ico");
    println!("cargo:rerun-if-changed=assets/icons/icon-256.png");

    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/super-platinum.ico");
        if let Err(err) = res.compile() {
            println!("cargo:warning=winresource failed to embed super-platinum.ico: {err}");
        }
    }
}
