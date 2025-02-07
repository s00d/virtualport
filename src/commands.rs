use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

/// Загружает команды из файла, где каждая команда и её ответ задаются в соседних строках.
pub fn load_commands_from_file(filename: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let path = Path::new(filename);

    if !path.exists() {
        eprintln!("[Warning] Command file '{}' not found. Running without predefined commands.", filename);
        return commands;
    }

    let file = File::open(filename).expect("Failed to open commands file");
    let reader = io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(Ok(command)) = lines.next() {
        if let Some(Ok(response)) = lines.next() {
            commands.insert(command, response);
        }
    }

    commands
}
