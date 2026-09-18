use crate::errors::EvmError;
use serde::Serialize;

pub fn render<T: Serialize>(value: &T) {
    write_json(value);
}

pub fn render_error(err: &EvmError) {
    let json = serde_json::json!({
        "error": err.machine_code(),
        "message": err.to_string(),
    });
    write_json(&json);
}

fn write_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => super::write_stdout(&json),
        Err(error) => {
            eprintln!("Error serializing JSON: {error}");
            std::process::exit(1);
        }
    }
}
