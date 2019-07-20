use std::fs::File;
use std::io::{BufReader, Cursor, Error, ErrorKind, Read, Seek, SeekFrom};
use std::path::PathBuf;

use byteorder::{LittleEndian, ReadBytesExt};
use linked_hash_map::LinkedHashMap;
use serde::Deserialize;
use docopt::Docopt;
use armake2::p3d::{P3D, LOD, Face, Vertex, Point};

mod io;
use crate::io::ReadExt;

pub const USAGE: &'static str = "
crowbar

Usage:
    crowbar <input> [<output>]
    crowbar (-h | --help)
    crowbar --version

Options:
    -h --help                   Show usage information and exit.
       --version                Print the version number and exit.
";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct Args {
    flag_version: bool,
    arg_input: PathBuf,
    arg_output: Option<PathBuf>,
}

fn read_compressed_array<I: Read + Seek>(reader: &mut I, output_size: usize) -> Result<Vec<u8>, Error> {
    let comp_type = reader.read_u8()?;
    if comp_type == 0 {
        let mut buffer = Vec::with_capacity(output_size);
        buffer.resize(output_size, 0);
        reader.read_exact(&mut buffer)?;
        return Ok(buffer);
    }

    assert_eq!(comp_type, 2);

    let fp = reader.seek(SeekFrom::Current(0))?;
    let mut size_small = 0;
    let mut size_large = output_size;

    // TODO: super hacky
    loop {
        if size_large < size_small || size_large - size_small <= 1 {
            return Err(Error::new(ErrorKind::Other, ""));
        }

        let size = size_small + (size_large - size_small) / 2;
        println!("    guessing LZO size: {:?} ({} - {})", size, size_small, size_large);

        //let mut buffer = [0; size];
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, 0);

        reader.seek(SeekFrom::Start(fp))?;
        let result = reader.read_exact(&mut buffer);

        if let Err(e) = result {
            if e.kind() == ErrorKind::UnexpectedEof {
                size_large = size;
                continue;
            } else {
                return Err(e);
            }
        }

        let result = minilzo::decompress(&buffer, output_size);
        match result {
            Ok(decomp) => {
                return Ok(decomp);
            },
            Err(minilzo::Error::InputOverrun) => {
                size_small = size;
            },
            Err(minilzo::Error::InputNotConsumed) => {
                size_large = size;
            },
            Err(e) => {
                return Err(Error::new(ErrorKind::Other, format!("{:?}", e)));
            }
        }
    }
}

fn read_odol(path: PathBuf) -> Result<P3D, Error> {
    let mut reader = BufReader::new(File::open(path)?);

    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)?;
    assert_eq!(&buffer, b"ODOL");

    let version = reader.read_u32::<LittleEndian>()?;
    println!("version: {}", version);

    assert_eq!(version, 73);

    let appid = reader.read_u32::<LittleEndian>()?;
    println!("appid: {}", appid);

    let muzzleflash = reader.read_cstring()?;
    println!("muzzleflash: \"{}\"", muzzleflash);

    let num_lods = reader.read_u32::<LittleEndian>()?;
    println!("num lods: {}", num_lods);

    let mut lods = Vec::new();

    for _i in 0..num_lods {
        let lod = LOD {
            version_major: 28,
            version_minor: 256,
            resolution: reader.read_f32::<LittleEndian>()?,
            points: Vec::new(),
            face_normals: Vec::new(),
            faces: Vec::new(),
            taggs: LinkedHashMap::new(),
        };

        println!("  - {}", lod.resolution);
        lods.push(lod);
    }

    println!("index: 0x{:x}", reader.read_u32::<LittleEndian>()?);

    println!("mem lod sphere: {}", reader.read_f32::<LittleEndian>()?);
    println!("geo lod sphere: {}", reader.read_f32::<LittleEndian>()?);

    println!("point flags: {:x}, {:x}, {:x}",
        reader.read_u32::<LittleEndian>()?,
        reader.read_u32::<LittleEndian>()?,
        reader.read_u32::<LittleEndian>()?);

    let offset_1 = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("offset 1: {:?}", offset_1);

    println!("map icon color: {:x}", reader.read_u32::<LittleEndian>()?);
    println!("map selected color: {:x}", reader.read_u32::<LittleEndian>()?);

    let view_density = reader.read_f32::<LittleEndian>()?;
    println!("view density: {}", view_density);

    let bbox_min = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    let bbox_max = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("bounding box: {:?} - {:?}", bbox_min, bbox_max);

    println!("lod density coef: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("draw importance: {:?}", reader.read_f32::<LittleEndian>()?);

    let bbox_visual_min = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    let bbox_visual_max = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("bounding box visual: {:?} - {:?}", bbox_visual_min, bbox_visual_max);

    let bounding_center = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("bounding center: {:?}", bounding_center);

    let geometry_center = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("geometry center: {:?}", geometry_center);

    let cog_offset = (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?);
    println!("cog offset: {:?}", cog_offset);

    println!("inv inertia: {:?}", (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?));
    println!("             {:?}", (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?));
    println!("             {:?}", (
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?));

    println!("autocenter: 0x{:x}", reader.read_u8()?);
    println!("lock autocenter: 0x{:x}", reader.read_u8()?);
    println!("can occlude: 0x{:x}", reader.read_u8()?);
    println!("can be occluded: 0x{:x}", reader.read_u8()?);
    println!("ai cover: 0x{:x}", reader.read_u8()?);

    println!("skeleton ht min: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("skeleton ht max: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("skeleton af max: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("skeleton mf max: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("skeleton mf act: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("skeleton t body: {:?}", reader.read_f32::<LittleEndian>()?);

    println!("force not alpha: 0x{:x}", reader.read_u8()?);
    println!("sb source: {}", reader.read_i32::<LittleEndian>()?);
    println!("prefer shadow volume: 0x{:x}", reader.read_u8()?);
    println!("shadow offset: {}", reader.read_f32::<LittleEndian>()?);
    println!("animated: 0x{:x}", reader.read_u8()?);

    let skeleton_name = reader.read_cstring()?;
    println!("skeleton name: \"{}\"", skeleton_name);

    if skeleton_name != "" {
        reader.seek(SeekFrom::Current(1))?;
        let num_bones = reader.read_u32::<LittleEndian>()?;
        println!("num bones: {}", num_bones);
        for _i in 0..num_bones {
            let name = reader.read_cstring()?;
            let parent = reader.read_cstring()?;
            println!("  - {} -> {}", name, parent);
        }
        assert_eq!(reader.read_u8()?, 0);
    }

    println!("map type: 0x{:x}", reader.read_u8()?);

    let num_floats = reader.read_u32::<LittleEndian>()?;
    println!("num floats: {}", num_floats);
    reader.seek(SeekFrom::Current((num_floats * 4) as i64))?;

    println!("mass: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("mass inv: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("armor: {:?}", reader.read_f32::<LittleEndian>()?);
    println!("armor inv: {:?}", reader.read_f32::<LittleEndian>()?);

    println!("lod indices:");
    println!("  memory: {}", reader.read_i8()?);
    println!("  geometry: {}", reader.read_i8()?);
    println!("  geometry simple: {}", reader.read_i8()?);
    println!("  geometry physx: {}", reader.read_i8()?);
    println!("  geometry fire: {}", reader.read_i8()?);
    println!("  geometry view: {}", reader.read_i8()?);
    println!("  geometry view pilot: {}", reader.read_i8()?);
    println!("  geometry view gunner: {}", reader.read_i8()?);
    println!("  geometry view commander: {}", reader.read_i8()?);
    println!("  geometry view cargo: {}", reader.read_i8()?);
    println!("  land contact: {}", reader.read_i8()?);
    println!("  roadway: {}", reader.read_i8()?);
    println!("  paths: {}", reader.read_i8()?);
    println!("  hitpoints: {}", reader.read_i8()?);

    reader.seek(SeekFrom::Current(4))?;

    println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

    println!("min shadow: {}", reader.read_u32::<LittleEndian>()?);
    println!("can blend: 0x{:x}", reader.read_u8()?);

    println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

    println!("class type: \"{}\"", reader.read_cstring()?);
    println!("destruct type: \"{}\"", reader.read_cstring()?);

    reader.seek(SeekFrom::Current((1 + 4) as i64))?;

    println!("lod defaults:");
    for _i in 0..num_lods {
        println!("  - {:x} {:x} {:x}",
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?);
    }
    println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

    let animations = reader.read_u8()?;
    if animations > 0 {
        let num_anims = reader.read_u32::<LittleEndian>()?;
        println!("  num anims: {}", num_anims);
        let mut animtypes: Vec<u32> = Vec::with_capacity(num_anims as usize);
        for _i in 0..num_anims {
            let animtype = reader.read_u32::<LittleEndian>()?;
            animtypes.push(animtype);
            println!("    - {}", reader.read_cstring()?);
            println!("      type: 0x{:x}", animtype);
            println!("      source: \"{}\"", reader.read_cstring()?);
            println!("      value: {:?} - {:?}",
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?);
            println!("      phase: {:?} - {:?}",
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?);
            reader.seek(SeekFrom::Current(4))?;
            //assert_eq!(reader.read_u32::<LittleEndian>()?, 0x38d1b717);
            assert_eq!(reader.read_u32::<LittleEndian>()?, 0);
            println!("      source address: {}", reader.read_u32::<LittleEndian>()?);

            if animtype <= 3 {
                println!("      angle: {:?} - {:?}",
                    reader.read_f32::<LittleEndian>()?,
                    reader.read_f32::<LittleEndian>()?);
            } else if animtype <= 7 {
                println!("      offset: {:?} - {:?}",
                    reader.read_f32::<LittleEndian>()?,
                    reader.read_f32::<LittleEndian>()?);
            } else if animtype == 8 {
                reader.seek(SeekFrom::Current(4*4))?;
            } else {
                println!("      hide: {:?}", reader.read_f32::<LittleEndian>()?);
                println!("      unhide: {:?}", reader.read_f32::<LittleEndian>()?);
            }
        }

        let num_resolutions = reader.read_u32::<LittleEndian>()?;
        println!("  num resolutions: {}", num_resolutions);
        for _i in 0..num_resolutions {
            let num_bones = reader.read_u32::<LittleEndian>()?;
            for _j in 0..num_bones {
                let num_anims = reader.read_u32::<LittleEndian>()?;
                reader.seek(SeekFrom::Current((num_anims * 4) as i64));
            }
        }
        for _i in 0..num_resolutions {
            for animtype in animtypes.iter() {
                let bone_name_index = reader.read_i32::<LittleEndian>()?;
                if bone_name_index != -1 && *animtype < 8 {
                    reader.seek(SeekFrom::Current(2 * 12));
                }
            }
        }
    }

    let mut lod_indices: Vec<u32> = Vec::with_capacity(num_lods as usize);
    for _i in 0..num_lods {
        lod_indices.push(reader.read_u32::<LittleEndian>()?);
    }

    println!("lod indices: {:?}", lod_indices);

    for (i, lod) in lods.iter_mut().enumerate() {
        println!("LOD {} (0x{:x})", lod.resolution, lod_indices[i]);
        reader.seek(SeekFrom::Start(lod_indices[i] as u64))?;

        let num_proxies = reader.read_u32::<LittleEndian>()?;
        println!("  num proxies: {}", num_proxies);
        for _i in 0..num_proxies {
            println!("    - {}", reader.read_cstring()?);
            reader.seek(SeekFrom::Current(4*12 + 4*4))?;
        }

        let num_bones_subskeleton = reader.read_u32::<LittleEndian>()?;
        println!("  num bones subskeleton: {}", num_bones_subskeleton);
        reader.seek(SeekFrom::Current((num_bones_subskeleton * 4) as i64))?;

        let num_bones_skeleton = reader.read_u32::<LittleEndian>()?;
        println!("  num bones skeleton: {}", num_bones_skeleton);
        for _i in 0..num_bones_skeleton {
            let num_links = reader.read_u32::<LittleEndian>()?;
            reader.seek(SeekFrom::Current((num_links * 4) as i64))?;
        }

        let num_points = reader.read_u32::<LittleEndian>()?;
        println!("  num points: {}", num_points);

        reader.seek(SeekFrom::Current(3*4 + 3*12 + 4))?;

        let num_textures = reader.read_u32::<LittleEndian>()?;
        println!("  num textures: {}", num_textures);
        let mut textures: Vec<String> = Vec::with_capacity(num_textures as usize);
        for _i in 0..num_textures {
            let texture = reader.read_cstring()?;
            println!("    - {}", texture);
            textures.push(texture);
        }

        let num_materials = reader.read_u32::<LittleEndian>()?;
        println!("  num materials: {}", num_materials);
        let mut materials: Vec<String> = Vec::with_capacity(num_materials as usize);
        for _i in 0..num_materials {
            let path = reader.read_cstring()?;
            println!("    - {}", path);

            reader.seek(SeekFrom::Current(4 + 6*16 + 5*4))?;

            let surface = reader.read_cstring()?;
            println!("      surface: \"{}\"", surface);

            reader.seek(SeekFrom::Current(2*4))?;
            let num_stages = reader.read_u32::<LittleEndian>()?;
            println!("      num stages: {}", num_stages);
            let num_transforms = reader.read_u32::<LittleEndian>()?;
            println!("      num transforms: {}", num_transforms);

            for _j in 0..num_stages {
                reader.seek(SeekFrom::Current(4))?;
                println!("        - {}", reader.read_cstring()?);
                reader.seek(SeekFrom::Current(4 + 1))?;
            }

            reader.seek(SeekFrom::Current((num_transforms * (4 + 3*4*4)) as i64))?;

            reader.seek(SeekFrom::Current(4))?;
            reader.read_cstring()?;
            reader.seek(SeekFrom::Current(4 + 1))?;

            materials.push(path);
        }

        let num_edges1 = reader.read_u32::<LittleEndian>()?;
        println!("  num edges 1: {}", num_edges1);
        reader.seek(SeekFrom::Current((2 * num_edges1) as i64))?;

        let num_edges2 = reader.read_u32::<LittleEndian>()?;
        println!("  num edges 2: {}", num_edges2);
        reader.seek(SeekFrom::Current((2 * num_edges2) as i64))?;

        //println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

        let num_faces = reader.read_u32::<LittleEndian>()?;
        println!("  num faces: {}", num_faces);
        reader.seek(SeekFrom::Current(6))?;
        let mut faces: Vec<(Vec<u32>, usize, usize)> = Vec::with_capacity(num_faces as usize);
        for _i in 0..num_faces {
            let face_type = reader.read_u8()?;
            let mut face: Vec<u32> = Vec::with_capacity(face_type as usize);
            for _j in 0..face_type {
                face.push(reader.read_u32::<LittleEndian>()?);
            }
            //println!("    {}: {:?}", face_type, face);
            faces.push((face, 0xffffff, 0xffffff));
        }

        //println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

        let num_sections = reader.read_u32::<LittleEndian>()?;
        println!("  num sections: {}", num_sections);
        let mut sections: Vec<(u32, u32)> = Vec::with_capacity(num_sections as usize);
        for _i in 0..num_sections {
            let face_from = reader.read_u32::<LittleEndian>()?;
            let face_to = reader.read_u32::<LittleEndian>()?;
            println!("    - {} - {}", face_from, face_to);
            sections.push((face_from, face_to));

            reader.seek(SeekFrom::Current(3*4))?;

            let texture_index = reader.read_u16::<LittleEndian>()?;
            println!("      texture index: {}", texture_index);

            reader.seek(SeekFrom::Current(4))?;

            let material_index = reader.read_i32::<LittleEndian>()?;
            println!("      material index: {}", material_index);
            if material_index == -1 {
                reader.seek(SeekFrom::Current(1))?;
            }

            let num_stages = reader.read_u32::<LittleEndian>()?;
            println!("      num stages: {}", num_stages);
            reader.seek(SeekFrom::Current((4*num_stages) as i64))?;

            let coll_info = reader.read_u32::<LittleEndian>()?;
            println!("      coll info: {}", coll_info);
            if coll_info > 0 {
                reader.seek(SeekFrom::Current(2*12 + 4 + 12 + 4))?;
            }

            let mut face_index = 0;
            for mut face in faces.iter_mut() {
                if face_index > face_to {
                    break;
                }

                if face_index >= face_from {
                    face.1 = texture_index as usize;
                    face.2 = material_index as usize;
                }

                face_index += face.0.len() as u32 * 4 + 1;
            }
        }

        // TODO: handle selections properly
        let num_selections = reader.read_u32::<LittleEndian>()?;
        println!("  num selections: {}", num_selections);
        let mut selections: Vec<(String, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u8>)> = Vec::with_capacity(num_selections as usize);
        for _i in 0..num_selections {
            let name = reader.read_cstring()?;
            println!("    - {}", name);

            let num_f = reader.read_u32::<LittleEndian>()?;
            println!("      num faces: {}", num_f);
            let mut faces: Vec<u32> = Vec::with_capacity(num_f as usize);
            if num_f > 0 {
                let mut cursor = Cursor::new(read_compressed_array(&mut reader, (num_f * 4) as usize)?);
                for _j in 0..num_f {
                    faces.push(cursor.read_u32::<LittleEndian>()?);
                }
            }

            let c = reader.read_u32::<LittleEndian>()?;
            reader.seek(SeekFrom::Current((c*4) as i64))?;

            reader.seek(SeekFrom::Current(1))?;

            let num_s = reader.read_u32::<LittleEndian>()?;
            println!("      num sections: {}", num_s);
            let mut sections: Vec<u32> = Vec::with_capacity(num_s as usize);
            if num_s > 0 {
                let mut cursor = Cursor::new(read_compressed_array(&mut reader, (num_s * 4) as usize)?);
                for _j in 0..num_s {
                    sections.push(cursor.read_u32::<LittleEndian>()?);
                }
            }

            let num_v = reader.read_u32::<LittleEndian>()?;
            println!("      num vertices: {}", num_v);
            let mut verts: Vec<u32> = Vec::with_capacity(num_v as usize);
            if num_v > 0 {
                let mut cursor = Cursor::new(read_compressed_array(&mut reader, (num_v * 4) as usize)?);
                for _j in 0..num_v {
                    verts.push(cursor.read_u32::<LittleEndian>()?);
                }
            }

            let num_w = reader.read_u32::<LittleEndian>()?;
            let vertweights: Vec<u8> = if num_w > 0 {
                read_compressed_array(&mut reader, num_w as usize)?
            } else {
                Vec::new()
            };

            selections.push((name, faces, sections, verts, vertweights));
        }

        let num_properties = reader.read_u32::<LittleEndian>()?;
        println!("  num properties: {}", num_properties);
        for _i in 0..num_properties {
            println!("    - {} = \"{}\"", reader.read_cstring()?, reader.read_cstring()?);
        }

        let num_frames = reader.read_u32::<LittleEndian>()?;
        assert_eq!(num_frames, 0);

        reader.seek(SeekFrom::Current(3*4 + 1 + 4))?;

        //println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

        let num_pointflags = reader.read_u32::<LittleEndian>()?;
        println!("  num pointflags: {}", num_pointflags);
        let comp_type = reader.read_u8()?;
        if comp_type == 1 {
            reader.seek(SeekFrom::Current(4))?;
        } else if comp_type == 0 {
            reader.seek(SeekFrom::Current((num_pointflags * 4) as i64))?;
        } else {
            unreachable!();
        }

        //println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);

        let uv_scale: (f32, f32, f32, f32) = (
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?);
        println!("  uv scale: ({}, {}, {}, {})", uv_scale.0, uv_scale.1, uv_scale.2, uv_scale.3);

        let uv_range: (f32, f32) = (uv_scale.2 - uv_scale.0, uv_scale.3 - uv_scale.1);

        // TODO: handle UVs properly
        let num_uvs = reader.read_u32::<LittleEndian>()?;
        println!("  num uvs: {}", num_uvs);
        let mut uvs: Vec<(f32, f32)> = Vec::with_capacity(num_uvs as usize);
        if num_uvs > 0 {
            let fill = reader.read_u8()?;
            if fill == 1 {
                let u: f32 = ((reader.read_i16::<LittleEndian>()? as i32 + 0x7fff) as f32) / ((2 * 0x7fff) as f32);
                let v: f32 = ((reader.read_i16::<LittleEndian>()? as i32 + 0x7fff) as f32) / ((2 * 0x7fff) as f32);
                let uv: (f32, f32) = (u * uv_range.0 + uv_scale.0, v * uv_range.1 + uv_scale.1);
                uvs.resize(num_uvs as usize, uv);
            } else if fill == 0 {
                let decompressed = read_compressed_array(&mut reader, (num_uvs * 4) as usize)?;
                let mut cursor = Cursor::new(decompressed);

                for i in 0..num_uvs {
                    let u: f32 = ((cursor.read_i16::<LittleEndian>()? as i32 + 0x7fff) as f32) / ((2 * 0x7fff) as f32);
                    let v: f32 = ((cursor.read_i16::<LittleEndian>()? as i32 + 0x7fff) as f32) / ((2 * 0x7fff) as f32);
                    let uv: (f32, f32) = (u * uv_range.0 + uv_scale.0, v * uv_range.1 + uv_scale.1);
                    if i < 20 {
                        println!("    - ({}, {})", uv.0, uv.1);
                    }
                    uvs.push(uv);
                }
            } else {
                unreachable!();
            }
        }

        let num_uvsets = reader.read_u32::<LittleEndian>()?;
        if num_uvs > 0 {
            for _i in 1..num_uvsets {
                let _uv_scale: (f32, f32, f32, f32) = (
                    reader.read_f32::<LittleEndian>()?,
                    reader.read_f32::<LittleEndian>()?,
                    reader.read_f32::<LittleEndian>()?,
                    reader.read_f32::<LittleEndian>()?);

                let num_uvs = reader.read_u32::<LittleEndian>()?;

                let fill = reader.read_u8()?;
                if fill == 1 {
                    reader.seek(SeekFrom::Current(4))?;
                } else if fill == 0 {
                    read_compressed_array(&mut reader, (num_uvs * 4) as usize)?;
                } else {
                    unreachable!();
                }
            }
        }

        assert_eq!(reader.read_u32::<LittleEndian>()?, num_points);
        println!("  num points: {}", num_points);
        let mut points: Vec<(f32, f32, f32)> = Vec::with_capacity(num_points as usize);
        if num_points > 0 {
            let decompressed = read_compressed_array(&mut reader, (num_points * 12) as usize)?;
            let mut cursor = Cursor::new(decompressed);

            for i in 0..num_points {
                let point = (cursor.read_f32::<LittleEndian>()?, cursor.read_f32::<LittleEndian>()?, cursor.read_f32::<LittleEndian>()?);
                if i < 20 {
                    println!("    - {:?}", point);
                }
                points.push(point);
            }
        }

        for p in points {
            lod.points.push(Point {
                coords: (
                    p.0 + bounding_center.0,
                    p.1 + bounding_center.1,
                    p.2 + bounding_center.2
                ),
                flags: 0
            });
            lod.face_normals.push((0.0, 0.0, 0.0)); // TODO
        }

        for (verts, t, m) in faces.iter() {
            let vertices: Vec<Vertex> = verts.iter().rev().map(|i| Vertex {
                point_index: *i,
                normal_index: *i,
                uv: uvs[*i as usize],
            }).collect();

            lod.faces.push(Face {
                vertices,
                flags: 0,
                texture: textures.get(*t).map(|t| t.clone()).unwrap_or(String::new()),
                material: materials.get(*m).map(|t| t.clone()).unwrap_or(String::new())
            })
        }

        for (name, selfaces, selsections, selverts, mut selvertweights) in selections {
            if selvertweights.len() == 0 {
                selvertweights = Vec::with_capacity(selverts.len());
                selvertweights.resize(selverts.len(), 0x1);
            }

            assert_eq!(selverts.len(), selvertweights.len());

            let mut mlod_verts: Vec<u8> = Vec::with_capacity(num_points as usize);
            let mut mlod_faces: Vec<u8> = Vec::with_capacity(num_faces as usize);
            mlod_verts.resize(num_points as usize, 0);
            mlod_faces.resize(num_faces as usize, 0);

            for i in selfaces {
                mlod_faces[i as usize] = 0x1;
            }

            for s in selsections {
                let section = sections[s as usize];
                for i in section.0..section.1 {
                    if i >= num_faces {
                        break;
                    }

                    mlod_faces[i as usize] = 0x1;
                    for j in faces[i as usize].0.iter() {
                        mlod_verts[*j as usize] = 0x1;
                    }
                }
            }

            for (i,w) in selverts.iter().zip(selvertweights.iter()) {
                mlod_verts[*i as usize] = *w;
            }

            mlod_verts.append(&mut mlod_faces);
            lod.taggs.insert(name, mlod_verts.into_boxed_slice());
        }
    }

    Ok(P3D {
        version: 257,
        lods: lods
    })
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        println!("v{}", VERSION);
        std::process::exit(0);
    }

    println!("{:?}", args);

    let mlod = read_odol(args.arg_input).expect("Failed to read ODOL");

    if let Some(output_path) = args.arg_output {
        let mut f = File::create(output_path).expect("Failed to open output.");
        mlod.write(&mut f).expect("Failed to write MLOD");
    }
}
