use std::path::PathBuf;

use pdfium_render::prelude::Pdfium;

// For the sake of simplicity, we only Support Mac (ARM64) and Linux (AMD 64-bit)
enum SupportArch {
    MacOSARM,
    LinuxAMD64,
    LinuxARM64,
}

fn _get_pdfium_instance(arch: SupportArch) -> Pdfium {
    let lib_arch = match arch {
        SupportArch::MacOSARM => "macos-arm",
        SupportArch::LinuxAMD64 => "linux-amd64",
        SupportArch::LinuxARM64 => "linux-arm64",
    };

    // Make sure that resources/pdfium/<arch>/lib is available in production
    let lib_path = std::env::current_dir().expect("Could not get the current dir path");

    let runtime_lib_path = lib_path
        .join("resources")
        .join("pdfium")
        .join(lib_arch)
        .join("lib");

    // When executing this library from Cargo, we must use
    // resources under the crate's folder
    let mut crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir.pop();
    let crate_dir = crate_dir
        .join("resources")
        .join("pdfium")
        .join(lib_arch)
        .join("lib");

    tracing::info!("Dynamic PDFium from '{runtime_lib_path:#?}'");

    Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            &runtime_lib_path,
        ))
        .or_else(|_| {
            tracing::info!("Trying to bind to crate-path directory of pdfium");
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&crate_dir))
        })
        .or_else(|_| {
            tracing::info!("Trying to bind to system pdfium");
            Pdfium::bind_to_system_library()
        })
        .unwrap(),
    )
}

/// Bind to the library at a specific path during runtime.
/// Panics if PDFium isn't available during runtime.
pub fn get_pdfium_instance() -> Pdfium {
    // We don't care about Intel Macs anymore...
    if cfg!(target_os = "macos") {
        return _get_pdfium_instance(SupportArch::MacOSARM);
    }

    if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        return _get_pdfium_instance(SupportArch::LinuxARM64);
    }

    // Sorry Windows folks...
    _get_pdfium_instance(SupportArch::LinuxAMD64)
}
