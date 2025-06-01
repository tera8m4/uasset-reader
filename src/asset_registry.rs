use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct AssetRegistryData {
    pub object_path: String,
    pub object_class_name: String,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct AssetData {
    pub asset_class_name: String,
    pub object_path_without_package_name: String,
    pub file_offset: i32,
}
