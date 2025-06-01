mod asset_registry;
mod data_table;
mod errors;
mod export_table;
mod exports;
mod parser;
mod property;
mod reader;
mod summary;
mod unreal_types;
mod versions;

use crate::parser::UassetParser;
use errors::ParseError;
use exports::ExportType;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), ParseError> {
    let args: Vec<String> = std::env::args().collect();

    let file_path = if args.len() > 1 {
        &args[1]
    } else {
        "../../../../../../../../../Templates/TP_InCamVFXBP/Content/InCamVFXBP/ExampleConfigs/nDisplayConfig_Curved.uasset"
    };

    let args_lower: Vec<String> = args.iter().map(|s| s.to_lowercase()).collect();

    let show_asset_registry = args_lower.contains(&"-assetregistry".to_string());
    let show_tags = args_lower.contains(&"-tags".to_string());
    let show_names = args_lower.contains(&"-names".to_string());
    let show_thumbnail_cache = args_lower.contains(&"-thumbnailcache".to_string());
    let show_data_tables = args_lower.contains(&"-datatables".to_string());

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut parser = UassetParser::new(reader, true)?;

    print_asset_data(
        &mut parser,
        show_asset_registry,
        show_tags,
        show_names,
        show_thumbnail_cache,
        show_data_tables,
    )?;

    Ok(())
}

fn print_asset_data(
    parser: &mut UassetParser<impl std::io::Read + std::io::Seek>,
    show_asset_registry: bool,
    show_tags: bool,
    show_names: bool,
    show_thumbnail_cache: bool,
    show_data_tables: bool,
) -> Result<(), ParseError> {
    // Print summary
    println!("{:#?}", parser.summary);

    if show_asset_registry {
        let registry_data = parser.get_asset_registry_data()?;
        for (idx, asset_data) in registry_data.iter().enumerate() {
            println!("\nAssetData {}\n", idx);
            println!("ObjectPath     : {}", asset_data.object_path);
            println!("ObjectClassName: {}", asset_data.object_class_name);

            if show_tags {
                println!("Tags");
                for (k, v) in &asset_data.tags {
                    println!("Tag {}: {}", k, v);
                }
            }
        }
    }

    if show_names {
        println!("\nNames\n");
        let names = &parser.get_names()?;
        for (idx, name) in names.iter().enumerate() {
            println!("Name {}: {}", idx, name);
        }
    }

    if show_thumbnail_cache {
        println!("\nThumbnailCache");
        let cache = parser.get_thumbnail_cache()?;
        for asset_data in cache {
            println!();
            println!(
                "AssetClassName              : {}",
                asset_data.asset_class_name
            );
            println!(
                "ObjectPathWithoutPackageName: {}",
                asset_data.object_path_without_package_name
            );
            println!("FileOffset                  : {}", asset_data.file_offset);
        }
    }

    // Read and display exports
    parser.read_exports()?;
    let exports = parser.get_exports();

    for (idx, export) in exports.iter().enumerate() {
        println!("\nExport {}: {:#?}", idx, export.entry);

        match &export.export_type {
            ExportType::Normal(data) => {
                println!("  Type: Normal Export");
                println!("  Data size: {} bytes", data.len());
            }
            ExportType::DataTable(dt) => {
                println!("  Type: DataTable Export");
                println!("  Properties: {} items", dt.properties.len());
                println!("  Table entries: {} rows", dt.table.data.len());

                if show_data_tables {
                    let names = &parser.names.as_ref().unwrap();
                    println!("  Property details:");
                    for prop in &dt.properties {
                        let prop_name =
                            if prop.name.index >= 0 && (prop.name.index as usize) < names.len() {
                                &names[prop.name.index as usize]
                            } else {
                                "InvalidName"
                            };
                        println!(
                            "    - {}: {} ({} bytes)",
                            prop_name,
                            prop.property_type,
                            prop.data.len()
                        );
                    }

                    println!("  Table row names:");
                    let row_names = dt.get_table_entry_names(names);
                    for (row_idx, row_name) in row_names.iter().enumerate() {
                        println!("    Row {}: {}", row_idx, row_name);
                    }
                }
            }
        }
    }

    Ok(())
}
