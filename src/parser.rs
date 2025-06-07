use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom};

use crate::asset_registry::{AssetData, AssetRegistryData};
use crate::errors::ParseError;
use crate::errors::Result;
use crate::export_table::ExportEntry;
use crate::reader::UassetReader;
use crate::summary::UassetSummary;
use crate::unreal_types::FName;
use crate::versions::EUnrealEngineObjectUE5Version;

pub struct UassetParser<R: Read + Seek> {
    reader: R,
    package_file_size: u64,
    allow_unversioned: bool,
    pub summary: UassetSummary,
    names: Option<Vec<String>>,
    asset_registry_data: Option<Vec<AssetRegistryData>>,
    thumbnail_cache: Option<Vec<AssetData>>,
    export: Vec<ExportEntry>,
}

impl<R: Read + Seek> UassetParser<R> {
    pub fn new(mut reader: R, allow_unversioned: bool) -> Result<Self> {
        let package_file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;

        let mut parser = UassetParser {
            reader,
            package_file_size,
            allow_unversioned,
            summary: UassetSummary::default(),
            names: None,
            asset_registry_data: None,
            thumbnail_cache: None,
            export: vec![],
        };

        parser.summary = parser.read_uasset_summary()?;
        Ok(parser)
    }

    pub fn get_names(&mut self) -> Result<&Vec<String>> {
        if self.names.is_none() {
            self.names = Some(self.read_names()?);
        }
        Ok(self.names.as_ref().unwrap())
    }

    pub fn get_asset_registry_data(&mut self) -> Result<&Vec<AssetRegistryData>> {
        if self.asset_registry_data.is_none() {
            self.asset_registry_data = Some(self.read_asset_registry_data()?);
        }
        Ok(self.asset_registry_data.as_ref().unwrap())
    }

    pub fn get_thumbnail_cache(&mut self) -> Result<&Vec<AssetData>> {
        if self.thumbnail_cache.is_none() {
            self.thumbnail_cache = Some(self.read_asset_data_from_thumbnail_cache()?);
        }
        Ok(self.thumbnail_cache.as_ref().unwrap())
    }

    pub fn get_exports(&self) -> &Vec<ExportEntry> {
        &self.export
    }

    fn check_file_offset(&self, offset: i64) -> Result<()> {
        if offset < 0 || offset as u64 > self.package_file_size {
            return Err(ParseError::InvalidFileOffset {
                offset,
                file_size: self.package_file_size,
            });
        }
        Ok(())
    }

    fn check_compression_flags(&self, flags: u32) -> Result<()> {
        const COMPRESS_DEPRECATED_FORMAT_FLAGS_MASK: u32 = 0x0F;
        const COMPRESS_OPTIONS_FLAGS_MASK: u32 = 0xF0;
        const COMPRESSION_FLAGS_MASK: u32 =
            COMPRESS_DEPRECATED_FORMAT_FLAGS_MASK | COMPRESS_OPTIONS_FLAGS_MASK;

        if flags & (!COMPRESSION_FLAGS_MASK) != 0 {
            return Err(ParseError::InvalidCompressionFlags);
        }
        Ok(())
    }

    fn check_asset_version(&self, major: u16, minor: u16, _patch: u16) -> Result<()> {
        const MIN_MAJOR: u16 = 4;
        const MIN_MINOR: u16 = 27;

        if major == 0 {
            if !self.allow_unversioned {
                return Err(ParseError::UnversionedAssetNotAllowed);
            }
        } else if major < MIN_MAJOR || (major == MIN_MAJOR && minor < MIN_MINOR) {
            return Err(ParseError::AssetVersionTooOld { major, minor });
        }
        Ok(())
    }

    fn read_uasset_summary(&mut self) -> Result<UassetSummary> {
        self.reader.seek(SeekFrom::Start(0))?;

        let mut s = UassetSummary::default();

        s.tag = self.reader.read_u32::<LittleEndian>()?;

        if s.tag != 0x9e2a83c1 {
            return Err(ParseError::InvalidTag);
        }

        s.legacy_file_version = self.reader.read_i32::<LittleEndian>()?;

        if ![-7, -8, -9].contains(&s.legacy_file_version) {
            return Err(ParseError::UnsupportedLegacyVersion(s.legacy_file_version));
        }

        s.legacy_ue3_version = self.reader.read_i32::<LittleEndian>()?;
        s.file_version_ue4 = self.reader.read_i32::<LittleEndian>()?;

        if s.legacy_file_version <= -8 {
            s.file_version_ue5 = self.reader.read_i32::<LittleEndian>()?;
        } else {
            s.file_version_ue5 = 0;
        }

        s.file_version_licensee_ue4 = self.reader.read_u32::<LittleEndian>()?;

        const KNOWN_SUPPORTED_UE5VER: i32 = 1017;
        if s.file_version_ue5 > KNOWN_SUPPORTED_UE5VER {
            eprintln!(
                "Warning: ObjectUE5Version {} too new; newest known supported version {}",
                s.file_version_ue5, KNOWN_SUPPORTED_UE5VER
            );
            eprintln!("Parsing will attempt to continue, but there may be errors reading the file");
        }

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            let mut hash = [0u8; 20];
            self.reader.read_exact(&mut hash)?;
            s.saved_hash = Some(hash);
            s.total_header_size = self.reader.read_i32::<LittleEndian>()?;
        }

        s.custom_versions = self.reader.read_tarray(
            |reader| {
                let mut buf = [0u8; 20];
                reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            100000,
        )?;

        if s.file_version_ue5 < EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            s.total_header_size = self.reader.read_i32::<LittleEndian>()?;
        }

        s.package_name = self.reader.read_fstring()?;
        s.package_flags = self.reader.read_u32::<LittleEndian>()?;
        s.name_count = self.reader.read_i32::<LittleEndian>()?;
        s.name_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::AddSoftObjectPathList as i32 {
            s.soft_object_paths_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.soft_object_paths_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        s.localization_id = self.reader.read_fstring()?;

        s.gatherable_text_data_count = self.reader.read_i32::<LittleEndian>()?;
        s.gatherable_text_data_offset = self.reader.read_i32::<LittleEndian>()?;
        s.export_count = self.reader.read_i32::<LittleEndian>()?;
        s.export_offset = self.reader.read_i32::<LittleEndian>()?;
        s.import_count = self.reader.read_i32::<LittleEndian>()?;
        s.import_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::VerseCells as i32 {
            s.cell_export_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_export_offset = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_import_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.cell_import_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::MetadataSerializationOffset as i32 {
            s.metadata_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        s.depends_offset = self.reader.read_i32::<LittleEndian>()?;
        s.soft_package_references_count = self.reader.read_i32::<LittleEndian>()?;
        s.soft_package_references_offset = self.reader.read_i32::<LittleEndian>()?;
        s.searchable_names_offset = self.reader.read_i32::<LittleEndian>()?;
        s.thumbnail_table_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 < EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            let mut guid = [0u8; 16];
            self.reader.read_exact(&mut guid)?;
            s.guid = Some(guid);
        }

        let mut persistent_guid = [0u8; 16];
        self.reader.read_exact(&mut persistent_guid)?;
        s.persistent_guid = persistent_guid;

        self.check_file_offset(s.gatherable_text_data_offset as i64)?;
        self.check_file_offset(s.export_offset as i64)?;
        self.check_file_offset(s.import_offset as i64)?;
        self.check_file_offset(s.depends_offset as i64)?;
        self.check_file_offset(s.soft_package_references_offset as i64)?;
        self.check_file_offset(s.searchable_names_offset as i64)?;
        self.check_file_offset(s.thumbnail_table_offset as i64)?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);
        let max_generations = (remaining_bytes / 20) as usize;

        s.generations = self.reader.read_tarray(
            |reader| {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            max_generations,
        )?;

        s.saved_by_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.saved_by_engine_version_name = self.reader.read_fstring()?;

        s.compatible_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.compatible_engine_version_name = self.reader.read_fstring()?;

        self.check_asset_version(
            s.saved_by_engine_version_major,
            s.saved_by_engine_version_minor,
            s.saved_by_engine_version_patch,
        )?;

        s.compression_flags = self.reader.read_u32::<LittleEndian>()?;
        self.check_compression_flags(s.compression_flags)?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);
        let max_chunks = (remaining_bytes / 16) as usize;

        s.compressed_chunks = self.reader.read_tarray(
            |reader| {
                let mut buf = [0u8; 16];
                reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            max_chunks,
        )?;

        if !s.compressed_chunks.is_empty() {
            return Err(ParseError::CompressedChunksNotSupported);
        }

        s.package_source = self.reader.read_u32::<LittleEndian>()?;

        let current_pos = self.reader.stream_position()?;
        let remaining_bytes = (s.total_header_size as u64).saturating_sub(current_pos + 1);

        s.additional_packages_to_cook = self
            .reader
            .read_tarray(|reader| reader.read_fstring(), remaining_bytes as usize)?;

        s.asset_registry_data_offset = self.reader.read_i32::<LittleEndian>()?;
        s.bulk_data_start_offset = self.reader.read_i64::<LittleEndian>()?;

        self.check_file_offset(s.asset_registry_data_offset as i64)?;
        self.check_file_offset(s.bulk_data_start_offset)?;

        Ok(s)
    }

    fn read_names(&mut self) -> Result<Vec<String>> {
        if self.summary.name_count <= 0 {
            return Ok(Vec::new());
        }

        let offset = self.summary.name_offset;
        if offset <= 0 || offset as u64 > self.package_file_size {
            return Ok(Vec::new());
        }

        self.reader.seek(SeekFrom::Start(offset as u64))?;

        let mut names = Vec::with_capacity(self.summary.name_count as usize);

        for _ in 0..self.summary.name_count {
            let name = self.reader.read_fstring()?;
            self.reader.skip_bytes(4)?; // Skip precalculated hashes
            names.push(name);
        }

        Ok(names)
    }

    fn read_fname(&mut self) -> Option<String> {
        let names = self.names.as_ref().unwrap();
        let fname = self.reader.read_fname().unwrap();
        if fname.is_none() {
            None
        } else {
            Some(names[fname.index as usize].clone())
        }
    }

    fn read_asset_registry_data(&mut self) -> Result<Vec<AssetRegistryData>> {
        let offset = self.summary.asset_registry_data_offset;

        if offset <= 0 || offset as u64 > self.package_file_size {
            return Ok(Vec::new());
        }

        self.reader.seek(SeekFrom::Start(offset as u64))?;

        let dependency_data_offset = self.reader.read_i64::<LittleEndian>()?;
        self.check_file_offset(dependency_data_offset)?;

        let n_assets = self.reader.read_i32::<LittleEndian>()?;

        if n_assets < 0 {
            return Err(ParseError::InvalidArraySize(n_assets));
        }

        let mut assets = Vec::with_capacity(n_assets as usize);

        for _ in 0..n_assets {
            let mut asset = AssetRegistryData::default();
            asset.object_path = self.reader.read_fstring()?;
            asset.object_class_name = self.reader.read_fstring()?;

            let n_tags = self.reader.read_i32::<LittleEndian>()?;

            for _ in 0..n_tags {
                match (self.reader.read_fstring(), self.reader.read_fstring()) {
                    (Ok(key), Ok(val)) => {
                        asset.tags.insert(key, val);
                    }
                    _ => {
                        assets.push(asset);
                        return Ok(assets);
                    }
                }
            }

            assets.push(asset);
        }

        Ok(assets)
    }

    fn read_asset_data_from_thumbnail_cache(&mut self) -> Result<Vec<AssetData>> {
        let offset = self.summary.thumbnail_table_offset;

        if offset <= 0 || offset as u64 > self.package_file_size {
            return Ok(Vec::new());
        }

        self.reader.seek(SeekFrom::Start(offset as u64))?;

        let object_count = self.reader.read_i32::<LittleEndian>()?;

        let mut asset_data_list = Vec::with_capacity(object_count as usize);

        for _ in 0..object_count {
            let mut asset_data = AssetData::default();

            asset_data.asset_class_name = self.reader.read_fstring()?;
            asset_data.object_path_without_package_name = self.reader.read_fstring()?;
            asset_data.file_offset = self.reader.read_i32::<LittleEndian>()?;

            asset_data_list.push(asset_data);
        }

        Ok(asset_data_list)
    }

    fn read_export(&mut self) -> Result<Vec<ExportEntry>> {
        let offset = self.summary.export_offset;
        let count = self.summary.export_count;

        if offset <= 0 || offset as u64 > self.package_file_size || count <= 0 {
            return Ok(Vec::new());
        }

        let mut entries: Vec<ExportEntry> = vec![];

        self.reader.seek(SeekFrom::Start(offset as u64))?;
        for _ in 0..count {
            let class_index = self.reader.read_i32::<LittleEndian>()?;
            let super_index = self.reader.read_i32::<LittleEndian>()?;
            let template_index = self.reader.read_i32::<LittleEndian>()?;
            let outer_index = self.reader.read_i32::<LittleEndian>()?;
            let object_name = self.reader.read_fname()?;
            let object_flags: i32 = self.reader.read_i32::<LittleEndian>()?;
            let serial_size: i64 = self.reader.read_i64::<LittleEndian>()?;
            let serial_offset: i64 = self.reader.read_i64::<LittleEndian>()?;

            let force_export = self.reader.read_u32::<LittleEndian>()? != 0;
            let not_for_client = self.reader.read_u32::<LittleEndian>()? != 0;
            let not_for_server = self.reader.read_u32::<LittleEndian>()? != 0;

            if self.summary.file_version_ue5
                < EUnrealEngineObjectUE5Version::RemoveObjectExportPackageGuid as i32
            {
                self.reader.read_i128::<LittleEndian>()?;
            }

            let is_inherited_instance = if self.summary.file_version_ue5
                > EUnrealEngineObjectUE5Version::TrackObjectExportIsInherited as i32
            {
                self.reader.read_u32::<LittleEndian>()? != 0
            } else {
                false
            };

            let package_flags = self.reader.read_u32::<LittleEndian>()?;
            let not_always_loaded_for_editor_game = self.reader.read_u32::<LittleEndian>()? != 0;
            let is_asset = self.reader.read_u32::<LittleEndian>()? != 0;

            let generate_public_hash = if self.summary.file_version_ue5
                >= EUnrealEngineObjectUE5Version::OptionalResources as i32
            {
                self.reader.read_u32::<LittleEndian>()? != 0
            } else {
                false
            };

            let first_export_dependency = self.reader.read_i32::<LittleEndian>()?;
            let serialization_before_serialization_dependencies =
                self.reader.read_i32::<LittleEndian>()?;
            let create_before_serialization_dependencies =
                self.reader.read_i32::<LittleEndian>()?;
            let serialization_before_create_dependencies =
                self.reader.read_i32::<LittleEndian>()?;
            let create_before_create_dependencies = self.reader.read_i32::<LittleEndian>()?;

            let script_serialization_start_offset = self.reader.read_i64::<LittleEndian>()?;
            let script_serialization_end_offset = self.reader.read_i64::<LittleEndian>()?;

            let entry = ExportEntry {
                class_index,
                super_index,
                template_index,
                outer_index,
                object_name,
                object_flags,
                serial_size,
                serial_offset,
                force_export,
                not_for_client,
                not_for_server,
                is_inherited_instance,
                package_flags,
                not_always_loaded_for_editor_game,
                is_asset,
                generate_public_hash,
                first_export_dependency,
                serialization_before_serialization_dependencies,
                create_before_serialization_dependencies,
                serialization_before_create_dependencies,
                create_before_create_dependencies,
                script_serialization_start_offset,
                script_serialization_end_offset,
            };

            entries.push(entry);
        }

        Ok(entries)
    }
}

pub fn print_asset_data(
    parser: &mut UassetParser<impl Read + Seek>,
    show_asset_registry: bool,
    show_tags: bool,
    show_names: bool,
    show_thumbnail_cache: bool,
) -> Result<()> {
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
        let names = parser.get_names()?;
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

    let exports = parser.read_export().unwrap();
    for export in &exports {
        println!("Export: {export:?}");
    }

    let data_table =&exports[1];

    println!("{}", &data_table.serial_offset);
    parser.reader.seek(SeekFrom::Start(data_table.serial_offset as u64))?;
    let flags = parser.reader.read_u8()?;

    loop {
        let tag = parser.read_fname();
        if tag.is_none() {
            break
        }
        let type_name = parser.read_fname().unwrap();
        let inner_count: i32 = parser.reader.read_i32::<LittleEndian>()?;

        println!("Name: {} Type: {type_name:?} : {inner_count}", &tag.as_ref().unwrap());
        let property_size = parser.reader.read_i32::<LittleEndian>()?;
        let property_flags = parser.reader.read_u8()?;

        let property_value = parser.reader.read_i32::<LittleEndian>()?;

        println!("Property size: {property_size}. flags: {property_flags}");
    }


    Ok(())
}
