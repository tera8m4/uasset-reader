use crate::unreal_types::FName;

#[derive(Debug, Clone)]
pub struct PropertyData {
    pub name: FName,
    pub property_type: String,
    pub data: Vec<u8>,
}

impl PropertyData {
    pub fn new(name: FName, property_type: String, data: Vec<u8>) -> Self {
        Self {
            name,
            property_type,
            data,
        }
    }
}
