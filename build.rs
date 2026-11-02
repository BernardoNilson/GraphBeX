//! Embeds the icon in the executable. The Windows taskbar reads it from there,
//! not from the icon the application sets on its window.

const ICON: &str = "src/modules/icon/GraphBeX Logo 512 transparent.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON}");

    #[cfg(windows)]
    embed_windows_icon();
}

#[cfg(windows)]
fn embed_windows_icon() {
    if let Err(error) = winresource::WindowsResource::new().set_icon(ICON).compile() {
        println!("cargo:warning=could not embed the icon: {error}");
    }
}
