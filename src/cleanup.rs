use std::fs::remove_file;

/// Структура для автоматической очистки созданного ресурса (например, символической ссылки).
pub struct Cleanup {
    pub link_path: String,
}

impl Cleanup {
    pub fn new(link_path: String) -> Self {
        Cleanup { link_path }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = remove_file(&self.link_path);
        println!("\n[Cleanup] Removed symbolic link: {}", self.link_path);
    }
}
