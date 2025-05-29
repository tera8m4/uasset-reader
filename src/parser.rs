use byteorder::{LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use thiserror::Error;

use crate::export_table::ExportEntry;
use crate::parser::EUnrealEngineObjectUE5Version::{OptionalResources, RemoveObjectExportPackageGuid, TrackObjectExportIsInherited};
use crate::unreal_types::FName;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid uasset tag")]
    InvalidTag,

    #[error("Unsupported legacy file version: {0}")]
    UnsupportedLegacyVersion(i32),

    #[error("Invalid file offset: {offset} (file size: {file_size})")]
    InvalidFileOffset { offset: i64, file_size: u64 },

    #[error("Invalid array size: {0}")]
    InvalidArraySize(i32),

    #[error("Invalid compression flags")]
    InvalidCompressionFlags,

    #[error("Compressed chunks not supported")]
    CompressedChunksNotSupported,

    #[error("Unversioned asset parsing not allowed")]
    UnversionedAssetNotAllowed,

    #[error("Asset version too old: {major}.{minor} (minimum: 4.27)")]
    AssetVersionTooOld { major: u16, minor: u16 },

    #[error("Invalid UTF-8 string")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid UTF-16 string")]
    InvalidUtf16,
}

type Result<T> = std::result::Result<T, ParseError>;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EUnrealEngineObjectUE5Version {
    InitialVersion = 1000,

    // Support stripping names that are not referenced from export data
    NamesReferencedFromExportData,

    // Added a payload table of contents to the package summary
    PayloadToc,

    // Added data to identify references from and to optional package
    OptionalResources,

    // Large world coordinates converts a number of core types to double components by default.
    LargeWorldCoordinates,

    // Remove package GUID from FObjectExport
    RemoveObjectExportPackageGuid,

    // Add IsInherited to the FObjectExport entry
    TrackObjectExportIsInherited,

    // Replace FName asset path in FSoftObjectPath with (package name, asset name) pair FTopLevelAssetPath
    FSoftObjectPathRemoveAssetPathFNames,

    // Add a soft object path list to the package summary for fast remap
    AddSoftObjectPathList,

    // Added bulk/data resource table
    DataResources,

    // Added script property serialization offset to export table entries for saved, versioned packages
    ScriptSerializationOffset,

    // Adding property tag extension,
    // Support for overridable serialization on UObject,
    // Support for overridable logic in containers
    PropertyTagExtensionAndOverridableSerialization,

    // Added property tag complete type name and serialization type
    PropertyTagCompleteTypeName,

    // Changed UE::AssetRegistry::WritePackageData to include PackageBuildDependencies
    AssetRegistryPackageBuildDependencies,

    // Added meta data serialization offset to for saved, versioned packages
    MetadataSerializationOffset,

    // Added VCells to the object graph
    VerseCells,

    // Changed PackageFileSummary to write FIoHash PackageSavedHash instead of FGuid Guid
    PackageSavedHash,

    // OS shadow serialization of subobjects
    OsSubObjectShadowSerialization,
}

#[derive(Debug, Default)]
pub struct UassetSummary {
    pub tag: u32,
    pub legacy_file_version: i32,
    pub legacy_ue3_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee_ue4: u32,
    pub saved_hash: Option<[u8; 20]>,
    pub total_header_size: i32,
    pub custom_versions: Vec<[u8; 20]>,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub soft_object_paths_count: Option<i32>,
    pub soft_object_paths_offset: Option<i32>,
    pub localization_id: String,
    pub gatherable_text_data_count: i32,
    pub gatherable_text_data_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub cell_export_count: Option<i32>,
    pub cell_export_offset: Option<i32>,
    pub cell_import_count: Option<i32>,
    pub cell_import_offset: Option<i32>,
    pub metadata_offset: Option<i32>,
    pub depends_offset: i32,
    pub soft_package_references_count: i32,
    pub soft_package_references_offset: i32,
    pub searchable_names_offset: i32,
    pub thumbnail_table_offset: i32,
    pub guid: Option<[u8; 16]>,
    pub persistent_guid: [u8; 16],
    pub generations: Vec<[u8; 8]>,
    pub saved_by_engine_version_major: u16,
    pub saved_by_engine_version_minor: u16,
    pub saved_by_engine_version_patch: u16,
    pub saved_by_engine_version_changelist: u32,
    pub saved_by_engine_version_name: String,
    pub compatible_engine_version_major: u16,
    pub compatible_engine_version_minor: u16,
    pub compatible_engine_version_patch: u16,
    pub compatible_engine_version_changelist: u32,
    pub compatible_engine_version_name: String,
    pub compression_flags: u32,
    pub compressed_chunks: Vec<[u8; 16]>,
    pub package_source: u32,
    pub additional_packages_to_cook: Vec<String>,
    pub asset_registry_data_offset: i32,
    pub bulk_data_start_offset: i64,
}

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

    fn read_fname(&mut self) -> Result<FName> {
        let index = self.reader.read_i32::<LittleEndian>()?;
        let number = self.reader.read_i32::<LittleEndian>()?;

        Ok(FName { index, number })
    }

    fn read_fstring(&mut self) -> Result<String> {
        let size = self.reader.read_i32::<LittleEndian>()?;

        if size == 0 {
            return Ok(String::new());
        }

        let (load_ucs2_char, actual_size) = if size < 0 {
            (true, (-size) as usize)
        } else {
            (false, size as usize)
        };

        let byte_size = if load_ucs2_char {
            actual_size * 2
        } else {
            actual_size
        };

        let mut buffer = vec![0u8; byte_size];
        self.reader.read_exact(&mut buffer)?;

        // Remove null terminator
        if load_ucs2_char {
            buffer.truncate(byte_size - 2);
            // Convert UTF-16LE to String
            let u16_vec: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16(&u16_vec).map_err(|_| ParseError::InvalidUtf16)
        } else {
            buffer.truncate(byte_size - 1);
            String::from_utf8(buffer).map_err(|e| e.into())
        }
    }

    fn skip_bytes(&mut self, n: i64) -> Result<()> {
        self.reader.seek(SeekFrom::Current(n))?;
        Ok(())
    }

    // fn skip_tarray(&mut self, type_size: usize, max_elements: usize) -> Result<()> {
    //     let n = self.reader.read_i32::<LittleEndian>()?;
    //
    //     if n < 0 || n as usize > max_elements {
    //         return Err(ParseError::InvalidArraySize(n));
    //     }
    //
    //     self.skip_bytes((n as usize * type_size) as i64)?;
    //     Ok(())
    // }

    fn read_tarray<T, F>(&mut self, mut reader_fn: F, max_elements: usize) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>,
    {
        let n = self.reader.read_i32::<LittleEndian>()?;

        if n < 0 || n as usize > max_elements {
            return Err(ParseError::InvalidArraySize(n));
        }

        let mut array = Vec::with_capacity(n as usize);
        for _ in 0..n {
            array.push(reader_fn(self)?);
        }
        Ok(array)
    }

    // fn read_byte_array(&mut self, size: usize) -> Result<Vec<u8>> {
    //     let mut buffer = vec![0u8; size];
    //     self.reader.read_exact(&mut buffer)?;
    //     Ok(buffer)
    // }

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

        s.custom_versions = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 20];
                parser.reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            100000,
        )?;

        if s.file_version_ue5 < EUnrealEngineObjectUE5Version::PackageSavedHash as i32 {
            s.total_header_size = self.reader.read_i32::<LittleEndian>()?;
        }

        s.package_name = self.read_fstring()?;
        s.package_flags = self.reader.read_u32::<LittleEndian>()?;
        s.name_count = self.reader.read_i32::<LittleEndian>()?;
        s.name_offset = self.reader.read_i32::<LittleEndian>()?;

        if s.file_version_ue5 >= EUnrealEngineObjectUE5Version::AddSoftObjectPathList as i32 {
            s.soft_object_paths_count = Some(self.reader.read_i32::<LittleEndian>()?);
            s.soft_object_paths_offset = Some(self.reader.read_i32::<LittleEndian>()?);
        }

        s.localization_id = self.read_fstring()?;

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

        s.generations = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 8];
                parser.reader.read_exact(&mut buf)?;
                Ok(buf)
            },
            max_generations,
        )?;

        s.saved_by_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.saved_by_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.saved_by_engine_version_name = self.read_fstring()?;

        s.compatible_engine_version_major = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_minor = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_patch = self.reader.read_u16::<LittleEndian>()?;
        s.compatible_engine_version_changelist = self.reader.read_u32::<LittleEndian>()?;
        s.compatible_engine_version_name = self.read_fstring()?;

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

        s.compressed_chunks = self.read_tarray(
            |parser| {
                let mut buf = [0u8; 16];
                parser.reader.read_exact(&mut buf)?;
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

        s.additional_packages_to_cook =
            self.read_tarray(|parser| parser.read_fstring(), remaining_bytes as usize)?;

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
            let name = self.read_fstring()?;
            self.skip_bytes(4)?; // Skip precalculated hashes
            names.push(name);
        }

        Ok(names)
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
            asset.object_path = self.read_fstring()?;
            asset.object_class_name = self.read_fstring()?;

            let n_tags = self.reader.read_i32::<LittleEndian>()?;

            for _ in 0..n_tags {
                match (self.read_fstring(), self.read_fstring()) {
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

            asset_data.asset_class_name = self.read_fstring()?;
            asset_data.object_path_without_package_name = self.read_fstring()?;
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
            let object_name = self.read_fname()?;
            let object_flags: i32 = self.reader.read_i32::<LittleEndian>()?;
            let serial_size: i64 = self.reader.read_i64::<LittleEndian>()?;
            let serial_offset: i64 = self.reader.read_i64::<LittleEndian>()?;

            let force_export = self.reader.read_u32::<LittleEndian>()? != 0;
            let not_for_client = self.reader.read_u32::<LittleEndian>()? != 0;
            let not_for_server = self.reader.read_u32::<LittleEndian>()? != 0;

            if (self.summary.file_version_ue5 < RemoveObjectExportPackageGuid as i32)
            {
                self.reader.read_i128::<LittleEndian>()?;
            }

            let is_inherited_instance = if (self.summary.file_version_ue5 > TrackObjectExportIsInherited as i32)
            {
                self.reader.read_u32::<LittleEndian>()? != 0
            } else {
                false
            };


            let package_flags = self.reader.read_u32::<LittleEndian>()?;
            let not_always_loaded_for_editor_game = self.reader.read_u32::<LittleEndian>()? != 0;
            let is_asset = self.reader.read_u32::<LittleEndian>()? != 0;

            let generate_public_hash = if self.summary.file_version_ue5 >= OptionalResources as i32 {
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
    for export in exports {
        println!("Export: {export:?}");
    }

    Ok(())
}

