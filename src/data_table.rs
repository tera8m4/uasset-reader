use crate::errors::{ParseError, Result};
use crate::property::PropertyData;
use crate::unreal_types::FName;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, Write};

#[derive(Debug, Clone)]
pub struct StructPropertyData {
    pub name: FName,
    pub struct_type: FName,
    pub data: Vec<u8>, // Raw property data for now - could be expanded to parse individual properties
}

#[derive(Debug)]
pub struct UDataTable {
    pub data: Vec<StructPropertyData>,
}

impl UDataTable {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn with_data(data: Vec<StructPropertyData>) -> Self {
        Self { data }
    }
}

#[derive(Debug)]
pub struct DataTableExport {
    pub properties: Vec<PropertyData>,
    pub table: UDataTable,
}

impl DataTableExport {
    pub fn new() -> Self {
        Self {
            properties: Vec::new(),
            table: UDataTable::new(),
        }
    }

    /// Read DataTable export data from the stream
    pub fn read<R: Read + Seek>(&mut self, reader: &mut R, names: &[String]) -> Result<()> {
        // First, read the normal properties (similar to NormalExport)
        self.read_properties(reader, names)?;

        // Find the RowStruct property to determine the struct type
        let decided_struct_type = self.find_row_struct_type(names).unwrap_or_else(|| FName {
            index: self.get_generic_name_index(names),
            number: 0,
        });

        // Read the data table entries
        let num_entries = reader.read_i32::<LittleEndian>()?;

        if num_entries < 0 || num_entries > 100000 {
            // Reasonable sanity check
            return Err(ParseError::InvalidArraySize(num_entries));
        }

        self.table.data.reserve(num_entries as usize);

        for _ in 0..num_entries {
            let row_name = self.read_fname(reader)?;

            // For now, we'll read the struct data as raw bytes
            // In a full implementation, you'd parse the actual struct properties here
            let struct_data = self.read_struct_data(reader)?;

            let struct_property = StructPropertyData {
                name: row_name,
                struct_type: decided_struct_type.clone(),
                data: struct_data,
            };

            self.table.data.push(struct_property);
        }

        Ok(())
    }

    /// Read normal properties before the table data
    fn read_properties<R: Read + Seek>(&mut self, reader: &mut R, names: &[String]) -> Result<()> {
        // This is a simplified property reader - in a full implementation you'd parse all property types
        loop {
            let name = self.read_fname(reader)?;

            // Check for "None" terminator
            if self.is_none_name(&name, names) {
                break;
            }

            let property_type = self.read_fname(reader)?;
            let size = reader.read_i64::<LittleEndian>()?;

            if size < 0 || size > 1024 * 1024 * 100 {
                // 100MB sanity check
                return Err(ParseError::InvalidArraySize(size as i32));
            }

            let mut property_data = vec![0u8; size as usize];
            reader.read_exact(&mut property_data)?;

            self.properties.push(PropertyData {
                name,
                property_type: self.get_name_string(&property_type, names),
                data: property_data,
            });
        }

        Ok(())
    }

    /// Find the RowStruct property to determine the struct type for table rows
    fn find_row_struct_type(&self, names: &[String]) -> Option<FName> {
        for property in &self.properties {
            let property_name = self.get_name_string(&property.name, names);
            if property_name == "RowStruct" && property.property_type == "ObjectProperty" {
                // In a full implementation, you'd parse the ObjectProperty to get the actual struct type
                // For now, we'll return a placeholder
                return Some(FName {
                    index: 0,
                    number: 0,
                });
            }
        }
        None
    }

    /// Get the index for "Generic" name (fallback struct type)
    fn get_generic_name_index(&self, names: &[String]) -> i32 {
        names
            .iter()
            .position(|name| name == "Generic")
            .map(|pos| pos as i32)
            .unwrap_or(0)
    }

    /// Read struct data (simplified - would parse actual properties in full implementation)
    fn read_struct_data<R: Read + Seek>(&self, reader: &mut R) -> Result<Vec<u8>> {
        // This is a placeholder - in a real implementation you'd parse the struct properties
        // For now, we'll read until we find a "None" terminator or reach a reasonable size limit
        let mut data = Vec::new();
        let mut temp_buffer = [0u8; 1024];

        // Read some data (this is very simplified)
        let bytes_read = reader.read(&mut temp_buffer)?;
        data.extend_from_slice(&temp_buffer[..bytes_read]);

        Ok(data)
    }

    /// Read an FName from the stream
    fn read_fname<R: Read + Seek>(&self, reader: &mut R) -> Result<FName> {
        let index = reader.read_i32::<LittleEndian>()?;
        let number = reader.read_i32::<LittleEndian>()?;
        Ok(FName { index, number })
    }

    /// Check if an FName represents "None"
    fn is_none_name(&self, name: &FName, names: &[String]) -> bool {
        self.get_name_string(name, names) == "None"
    }

    /// Get the string representation of an FName
    fn get_name_string(&self, name: &FName, names: &[String]) -> String {
        if name.index >= 0 && (name.index as usize) < names.len() {
            names[name.index as usize].clone()
        } else {
            format!("InvalidName_{}", name.index)
        }
    }

    /// Write DataTable export data to a stream (for serialization)
    pub fn write<W: Write>(&self, writer: &mut W, names: &[String]) -> Result<()> {
        // Write normal properties first
        for property in &self.properties {
            self.write_fname(writer, &property.name)?;

            // Don't write "None" terminator yet
            if !self.is_none_name(&property.name, names) {
                // Write property type and size
                let property_type_fname = FName {
                    index: names
                        .iter()
                        .position(|name| name == &property.property_type)
                        .map(|pos| pos as i32)
                        .unwrap_or(0),
                    number: 0,
                };
                self.write_fname(writer, &property_type_fname)?;

                writer.write_all(&(property.data.len() as i64).to_le_bytes())?;
                writer.write_all(&property.data)?;
            }
        }

        // Write "None" terminator
        let none_fname = FName {
            index: names
                .iter()
                .position(|name| name == "None")
                .map(|pos| pos as i32)
                .unwrap_or(0),
            number: 0,
        };
        self.write_fname(writer, &none_fname)?;

        // Write table data
        writer.write_all(&(self.table.data.len() as i32).to_le_bytes())?;

        for entry in &self.table.data {
            self.write_fname(writer, &entry.name)?;
            writer.write_all(&entry.data)?;
        }

        Ok(())
    }

    /// Write an FName to the stream
    fn write_fname<W: Write>(&self, writer: &mut W, name: &FName) -> Result<()> {
        writer.write_all(&name.index.to_le_bytes())?;
        writer.write_all(&name.number.to_le_bytes())?;
        Ok(())
    }

    /// Get a property by name (similar to C# indexer)
    pub fn get_property(&self, name: &str, names: &[String]) -> Option<&PropertyData> {
        self.properties
            .iter()
            .find(|prop| self.get_name_string(&prop.name, names) == name)
    }

    /// Get a table entry by name
    pub fn get_table_entry(&self, name: &str, names: &[String]) -> Option<&StructPropertyData> {
        self.table
            .data
            .iter()
            .find(|entry| self.get_name_string(&entry.name, names) == name)
    }

    /// Get all table entry names
    pub fn get_table_entry_names(&self, names: &[String]) -> Vec<String> {
        self.table
            .data
            .iter()
            .map(|entry| self.get_name_string(&entry.name, names))
            .collect()
    }
}
