fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../launcher-gui/ui/icons/logo.ico");
        resource.compile().unwrap();
    }
}
