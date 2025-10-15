#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("icons/icon.ico");
    res.set("ProductName", "ChromaBridge");
    res.set("FileDescription", "ChromaBridge - Color Accessibility Tool");
    res.set("LegalCopyright", "© 2025 ChromaBridge Contributors");
    res.set("CompanyName", "ChromaBridge");
    res.set("OriginalFilename", "chromabridge.exe");

    if let Err(e) = res.compile() {
        eprintln!("Failed to compile Windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {
}
