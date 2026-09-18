use comfy_table::{ContentArrangement, Table};

pub trait Tableable {
    fn to_table(&self) -> Table;
}

pub fn render<T: Tableable>(value: &T) {
    let mut table = value.to_table();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    super::write_stdout(&table.to_string());
}
