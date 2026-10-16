fn main() {
    // o recurso do ícone é embutido aqui; sem isso o cargo reaproveita o ícone antigo
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build();
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
