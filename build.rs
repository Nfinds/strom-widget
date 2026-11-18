fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Strom-Widget");
        res.set("FileDescription", "Strom-Widget");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Could not embed the icon: {e}");
        }
    }
}
