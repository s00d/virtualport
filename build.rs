fn main() {
    // Указываем линковку с libutil, если цель - Android
    #[cfg(target_os = "android")]
    println!("cargo:rustc-link-lib=util");
}
