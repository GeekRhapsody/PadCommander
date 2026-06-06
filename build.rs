fn main() {
    println!("cargo:rerun-if-changed=assets/windows/app.ico");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/windows/app.ico");
        resource.set("FileDescription", "Handheld Commander");
        resource.set("ProductName", "Handheld Commander");
        resource.set("OriginalFilename", "handheld-commander.exe");

        if let Err(error) = resource.compile() {
            panic!("failed to compile Windows application resources: {error}");
        }
    }
}
