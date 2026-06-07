fn main() {
    println!("cargo:rerun-if-changed=assets/windows/app.ico");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/windows/app.ico");
        resource.set("FileDescription", "PadCommander");
        resource.set("ProductName", "PadCommander");
        resource.set("OriginalFilename", "pad-commander.exe");

        if let Err(error) = resource.compile() {
            panic!("failed to compile Windows application resources: {error}");
        }
    }
}
