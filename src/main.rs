mod parser;

use crate::parser::{print_asset_data, ParseError, UassetParser};
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

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut parser = UassetParser::new(reader, true)?;

    print_asset_data(
        &mut parser,
        show_asset_registry,
        show_tags,
        show_names,
        show_thumbnail_cache,
    )?;

    Ok(())
}

// Cargo.toml dependencies:
/*
[dependencies]
byteorder = "1.5"
thiserror = "1.0"
*/