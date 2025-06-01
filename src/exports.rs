use crate::data_table::DataTableExport;
use crate::export_table::ExportEntry;

// Enum to represent different export types
#[derive(Debug)]
pub enum ExportType {
    Normal(Vec<u8>), // Raw export data for normal exports
    DataTable(DataTableExport),
    // Other export types can be added here in the future
    // Level, Enum, Function, etc.
}

#[derive(Debug)]
pub struct ParsedExport {
    pub entry: ExportEntry,
    pub export_type: ExportType,
}

impl ParsedExport {
    pub fn new_normal(entry: ExportEntry, data: Vec<u8>) -> Self {
        Self {
            entry,
            export_type: ExportType::Normal(data),
        }
    }

    pub fn new_data_table(entry: ExportEntry, data_table: DataTableExport) -> Self {
        Self {
            entry,
            export_type: ExportType::DataTable(data_table),
        }
    }

    pub fn is_data_table(&self) -> bool {
        matches!(self.export_type, ExportType::DataTable(_))
    }

    pub fn as_data_table(&self) -> Option<&DataTableExport> {
        match &self.export_type {
            ExportType::DataTable(dt) => Some(dt),
            _ => None,
        }
    }

    pub fn as_data_table_mut(&mut self) -> Option<&mut DataTableExport> {
        match &mut self.export_type {
            ExportType::DataTable(dt) => Some(dt),
            _ => None,
        }
    }
}
