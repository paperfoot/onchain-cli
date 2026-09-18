pub mod json;
pub mod table;

use serde::Serialize;
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputFormat {
    Json,
    Table,
}

impl OutputFormat {
    pub fn detect(json_flag: bool) -> Self {
        if json_flag || !std::io::stdout().is_terminal() {
            Self::Json
        } else {
            Self::Table
        }
    }
}

pub fn render<T: Serialize + table::Tableable>(value: &T, format: OutputFormat) {
    match format {
        OutputFormat::Json => json::render(value),
        OutputFormat::Table => table::render(value),
    }
}

pub fn write_stdout(text: &str) {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let result = stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.write_all(b"\n"))
        .and_then(|()| stdout.flush());

    if let Err(error) = result {
        if error.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("Error writing output: {error}");
        std::process::exit(1);
    }
}

pub fn render_error(err: &crate::errors::EvmError, format: OutputFormat) {
    match format {
        OutputFormat::Json => json::render_error(err),
        OutputFormat::Table => {
            eprintln!("Error: {err}");
        }
    }
}
