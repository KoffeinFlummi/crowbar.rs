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

    // skip rest of model config
    reader.seek(SeekFrom::Current(216))?;

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
        reader.seek(SeekFrom::Current(1))?;
    }

    println!("0x{:x}", reader.seek(SeekFrom::Current(0))?);
    reader.seek(SeekFrom::Current((1 + 4 + 4*4 + 14 + 4 + 1 + 2*1 + 1 + 4 + 4 + 12*num_lods) as i64))?;

    let animations = reader.read_u8()?;
    assert_eq!(animations, 0);

    let mut lod_indices: Vec<u32> = Vec::with_capacity(num_lods as usize);
    for _i in 0..num_lods {
        lod_indices.push(reader.read_u32::<LittleEndian>()?);
    }

    println!("lod indices: {:?}", lod_indices);

    for (i, lod) in lods.iter_mut().enumerate() {
        println!("LOD {}", lod.resolution);
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
        for _i in 0..num_sections {
            let face_from = reader.read_u32::<LittleEndian>()?;
            let face_to = reader.read_u32::<LittleEndian>()?;
            println!("    - {} - {}", face_from, face_to);

            reader.seek(SeekFrom::Current(3*4))?;

            let texture_index = reader.read_u16::<LittleEndian>()?;
            println!("    texture index: {}", texture_index);

            reader.seek(SeekFrom::Current(4))?;

            let material_index = reader.read_i32::<LittleEndian>()?;
            println!("    material index: {}", material_index);
            if material_index == -1 {
                reader.seek(SeekFrom::Current(1))?;
            }

            let num_stages = reader.read_u32::<LittleEndian>()?;
            println!("    num stages: {}", num_stages);
            reader.seek(SeekFrom::Current((4*num_stages) as i64))?;

            let coll_info = reader.read_u32::<LittleEndian>()?;
            println!("    coll info: {}", coll_info);
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
        for _i in 0..num_selections {
            let name = reader.read_cstring()?;
            println!("    - {}", name);

            let num_f = reader.read_u32::<LittleEndian>()?;
            if num_f > 0 {
                reader.seek(SeekFrom::Current((1 + 4*num_f) as i64))?;
            }

            reader.seek(SeekFrom::Current(5))?;

            let num_s = reader.read_u32::<LittleEndian>()?;
            if num_s > 0 {
                reader.seek(SeekFrom::Current((1 + 4*num_s) as i64))?;
            }

            let num_v = reader.read_u32::<LittleEndian>()?;
            if num_v > 0 {
                reader.seek(SeekFrom::Current((1 + 4*num_v) as i64))?;
            }

            let num_w = reader.read_u32::<LittleEndian>()?;
            if num_w > 0 {
                reader.seek(SeekFrom::Current((1 + num_w) as i64))?;
            }
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

        reader.seek(SeekFrom::Current(4))?;

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
                coords: p,
                flags: 0
            });
            lod.face_normals.push((0.0, 0.0, 0.0)); // TODO
        }

        for (verts, t, m) in faces {
            let vertices: Vec<Vertex> = verts.iter().map(|i| Vertex {
                point_index: *i,
                normal_index: *i,
                uv: uvs[*i as usize],
            }).collect();

            lod.faces.push(Face {
                vertices,
                flags: 0,
                texture: textures.get(t).map(|t| t.clone()).unwrap_or(String::new()),
                material: materials.get(m).map(|t| t.clone()).unwrap_or(String::new())
            })
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