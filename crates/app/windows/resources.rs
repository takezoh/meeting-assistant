//! The Windows resources of the executable, written as a compiled resource (.res) file: version information, the app
//! icon and the application manifest. The MSVC linker takes a .res file as an input, so the build needs no resource
//! compiler. Used by `build.rs`, which runs on the build machine, so this file depends on nothing but `std`.

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const RT_MANIFEST: u16 = 24;
/// English (United States), the language the version strings are written in.
const LANGUAGE: u16 = 0x0409;
/// UTF-16, the code page of the version strings.
const CODE_PAGE: u16 = 1200;

/// The sizes Windows picks the icon from: small and large icons at 100 % to 200 % scaling, and Explorer's largest view.
const ICON_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// What the version information says of the app.
pub struct Product<'a> {
    pub name: &'a str,
    /// The version as Cargo has it, such as `0.1.0`.
    pub version: &'a str,
    /// Major, minor, patch and a fourth number, which Windows shows as `0.1.0.0`.
    pub numbers: [u16; 4],
    /// Whether the version is a pre-release, such as `0.2.0-beta.1`.
    pub prerelease: bool,
    /// The executable's file name, which the installer keeps whatever the downloaded file was called.
    pub executable: &'a str,
}

/// The resource file: the icon in every size, the version information and the manifest.
pub fn res_file(product: &Product, manifest: &str) -> Vec<u8> {
    let mut out = Vec::new();
    // A .res file opens with an empty entry, which marks it as a 32-bit resource file.
    entry(&mut out, 0, 0, 0, &[]);

    let mut group = Vec::new();
    for value in [0u16, 1, ICON_SIZES.len() as u16] {
        group.extend(value.to_le_bytes());
    }
    for (index, &size) in ICON_SIZES.iter().enumerate() {
        let id = index as u16 + 1;
        let image = icon_image(size);
        entry(&mut out, RT_ICON, id, LANGUAGE, &image);
        // A width and height of 0 stand for 256.
        let side = (size % 256) as u8;
        group.extend([side, side, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((image.len() as u32).to_le_bytes());
        group.extend(id.to_le_bytes());
    }
    entry(&mut out, RT_GROUP_ICON, 1, LANGUAGE, &group);
    entry(&mut out, RT_VERSION, 1, LANGUAGE, &version_info(product));
    // ID 1 is the manifest Windows reads when it starts the executable.
    entry(&mut out, RT_MANIFEST, 1, LANGUAGE, manifest.as_bytes());
    out
}

/// Appends one resource: a header naming its type and ID by number, then its data, padded to 32 bits.
fn entry(out: &mut Vec<u8>, kind: u16, id: u16, language: u16, data: &[u8]) {
    const HEADER_SIZE: u32 = 32;
    // Moveable and pure, as the resource compiler marks them; Windows no longer acts on these flags.
    let flags: u16 = if kind == 0 { 0 } else { 0x0030 };
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(HEADER_SIZE.to_le_bytes());
    for value in [0xFFFF, kind, 0xFFFF, id] {
        out.extend(value.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.extend(language.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(data);
    align(out);
}

fn align(out: &mut Vec<u8>) {
    out.resize(out.len().next_multiple_of(4), 0);
}

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

/// The version information (`VS_VERSIONINFO`): the numbers Explorer shows under Details, then the strings, in
/// English with UTF-16 text.
fn version_info(product: &Product) -> Vec<u8> {
    const VS_FF_PRERELEASE: u32 = 0x2;
    const VOS_NT_WINDOWS32: u32 = 0x0004_0004;
    const VFT_APP: u32 = 1;

    let [major, minor, patch, build] = product.numbers.map(u32::from);
    let most = (major << 16) | minor;
    let least = (patch << 16) | build;
    let flags = if product.prerelease { VS_FF_PRERELEASE } else { 0 };
    // VS_FIXEDFILEINFO: signature, structure version, file version, product version, flag mask, flags, OS, file type,
    // subtype and a date that is not set.
    let fixed: Vec<u8> = [
        0xFEEF_04BD,
        0x0001_0000,
        most,
        least,
        most,
        least,
        0x3F,
        flags,
        VOS_NT_WINDOWS32,
        VFT_APP,
        0,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect();

    let internal_name = product.executable.trim_end_matches(".exe");
    let strings = [
        ("FileDescription", product.name),
        ("FileVersion", product.version),
        ("InternalName", internal_name),
        ("OriginalFilename", product.executable),
        ("ProductName", product.name),
        ("ProductVersion", product.version),
    ]
    .map(|(key, value)| text(key, value));
    let table = block(&format!("{LANGUAGE:04X}{CODE_PAGE:04X}"), &strings);
    let translation: Vec<u8> = [LANGUAGE, CODE_PAGE].into_iter().flat_map(u16::to_le_bytes).collect();
    let translation = node("Translation", 0, translation.len() as u16, &translation, &[]);
    node(
        "VS_VERSION_INFO",
        0,
        fixed.len() as u16,
        &fixed,
        &[block("StringFileInfo", &[table]), block("VarFileInfo", &[translation])],
    )
}

/// One node of the version information: its length, the length of its value, whether the value is text, and its
/// key, followed by the value and the child nodes, each starting on a 32-bit boundary.
fn node(key: &str, kind: u16, value_length: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend(value_length.to_le_bytes());
    out.extend(kind.to_le_bytes());
    out.extend(utf16z(key));
    align(&mut out);
    out.extend(value);
    for child in children {
        align(&mut out);
        out.extend(child);
    }
    let length = u16::try_from(out.len()).expect("the version information is far below 64 KiB");
    out[..2].copy_from_slice(&length.to_le_bytes());
    out
}

/// A node that holds only other nodes.
fn block(key: &str, children: &[Vec<u8>]) -> Vec<u8> {
    node(key, 1, 0, &[], children)
}

/// A string, whose value length counts UTF-16 units with the terminating zero.
fn text(key: &str, value: &str) -> Vec<u8> {
    let value = utf16z(value);
    node(key, 1, (value.len() / 2) as u16, &value, &[])
}

/// An icon image as Windows stores it: a bitmap header whose height counts the picture and its mask together, the
/// picture's rows from the bottom up in blue, green, red and alpha, then a mask with a bit set for each transparent
/// pixel, its rows padded to 32 bits.
fn icon_image(size: u32) -> Vec<u8> {
    let pixels = icon_pixels(size);
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for value in [40, size, size * 2] {
        out.extend(value.to_le_bytes());
    }
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((size * size * 4 + mask_row * size).to_le_bytes());
    out.extend([0; 16]);

    let rows: Vec<&[[u8; 4]]> = pixels.chunks(size as usize).rev().collect();
    for row in &rows {
        for &[red, green, blue, alpha] in *row {
            out.extend([blue, green, red, alpha]);
        }
    }
    for row in &rows {
        let mut mask = vec![0u8; mask_row as usize];
        for (x, pixel) in row.iter().enumerate() {
            if pixel[3] == 0 {
                mask[x / 8] |= 0x80 >> (x % 8);
            }
        }
        out.extend(mask);
    }
    out
}

/// The app's icon at one size, as rows from the top in red, green, blue and alpha: a red dot, the color the
/// notification-area icon takes while recording, inside a light ring on a dark disc, so that it reads on light and
/// dark backgrounds alike. Each pixel averages 4 by 4 samples, which smooths the edges.
fn icon_pixels(size: u32) -> Vec<[u8; 4]> {
    const SAMPLES: u32 = 4;
    const RED: [f32; 3] = [224.0, 45.0, 38.0];
    const LIGHT: [f32; 3] = [245.0, 245.0, 245.0];
    const DARK: [f32; 3] = [38.0, 38.0, 42.0];
    // Radii as fractions of the disc's.
    const DOT: f32 = 0.40;
    const RING: (f32, f32) = (0.62, 0.80);

    let side = size as f32;
    let center = side / 2.0;
    let radius = center - (side / 32.0).max(0.5);
    let color_at = |x: f32, y: f32| {
        let distance = ((x - center).powi(2) + (y - center).powi(2)).sqrt() / radius;
        if distance > 1.0 {
            None
        } else if distance <= DOT {
            Some(RED)
        } else if (RING.0..=RING.1).contains(&distance) {
            Some(LIGHT)
        } else {
            Some(DARK)
        }
    };

    let mut pixels = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 3];
            let mut covered = 0u32;
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let sample_x = x as f32 + (i as f32 + 0.5) / SAMPLES as f32;
                    let sample_y = y as f32 + (j as f32 + 0.5) / SAMPLES as f32;
                    if let Some(color) = color_at(sample_x, sample_y) {
                        covered += 1;
                        for (total, channel) in sum.iter_mut().zip(color) {
                            *total += channel;
                        }
                    }
                }
            }
            pixels.push(if covered == 0 {
                [0; 4]
            } else {
                let [red, green, blue] = sum.map(|total| (total / covered as f32).round() as u8);
                let alpha = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
                [red, green, blue, alpha]
            });
        }
    }
    pixels
}
