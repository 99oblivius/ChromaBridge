#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("../../assets/icons/icon.ico");
    res.set("ProductName", "Color Interlacer Overlay");
    res.set("FileDescription", "Color Interlacer Overlay - Rendering Process");
    res.set("LegalCopyright", "© 2025 Color Interlacer Contributors");
    res.set("CompanyName", "Color Interlacer");
    res.set("OriginalFilename", "color-interlacer-overlay.exe");

    if let Err(e) = res.compile() {
        eprintln!("Failed to compile Windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {
    // No-op on non-Windows platforms
}
