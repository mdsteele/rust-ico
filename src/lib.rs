//! A library for encoding/decoding [ICO image
//! files](https://en.wikipedia.org/wiki/ICO_%28file_format%29).
//!
//! # Overview
//!
//! An ICO file (.ico) stores a collection of small images of different sizes
//! and color depths (up to 256x256 pixels each).  Individial images within the
//! file can be encoded in either BMP or PNG format.  ICO files are typically
//! used for website favicons and for Windows application icons.
//!
//! CUR files (.cur), which store Windows cursor images, use the same file
//! format as ICO files, except that each image also comes with (x, y)
//! *hotspot* coordinates that determines where on the image the user is
//! pointing.  This libary supports both file types.
//!
//! # Examples
//!
//! ## Reading an ICO file
//!
//! ```no_run
//! // Read an ICO file from disk:
//! let file = std::fs::File::open("path/to/file.ico").unwrap();
//! let icon_dir = ico::IconDir::read(file).unwrap();
//! // Print the size of each image in the ICO file:
//! for entry in icon_dir.entries() {
//!     println!("{}x{}", entry.width(), entry.height());
//! }
//! // Decode the first entry into an image:
//! let image = icon_dir.entries()[0].decode().unwrap();
//! // You can get raw RGBA pixel data to pass to another image library:
//! let rgba = image.rgba_data();
//! assert_eq!(rgba.len(), (4 * image.width() * image.height()) as usize);
//! // Alternatively, you can save the image as a PNG file:
//! let file = std::fs::File::create("icon.png").unwrap();
//! image.to_png(file).unwrap();
//! ```
//!
//! ## Creating an ICO file
//!
//! ```no_run
//! // Create a new, empty icon collection:
//! let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
//! // Read a PNG file from disk and add it to the collection:
//! let file = std::fs::File::open("path/to/image.png").unwrap();
//! let image = ico::IconImage::from_png(file).unwrap();
//! icon_dir.add_entry(image).unwrap();
//! // Alternatively, you can create an IconImage from raw RGBA pixel data
//! // (e.g. from another image library):
//! let rgba = vec![std::u8::MAX; 4 * 16 * 16];
//! let image = ico::IconImage::from_rgba_data(16, 16, rgba);
//! icon_dir.add_entry(image).unwrap();
//! // Finally, write the ICO file to disk:
//! let file = std::fs::File::create("favicon.ico").unwrap();
//! icon_dir.write(file).unwrap();
//! ```

#![warn(missing_docs)]

extern crate byteorder;
extern crate png;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use png::HasParameters;
use std::{u16, u8};
use std::io::{self, Read, Seek, SeekFrom, Write};

// ========================================================================= //

// The size of a BITMAPINFOHEADER struct, in bytes.
const BMP_HEADER_LEN: u32 = 40;

// The signature that all PNG files start with.
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G'];

// Size limits for images in an ICO file:
const MIN_WIDTH: u32 = 1;
const MAX_WIDTH: u32 = 256;
const MIN_HEIGHT: u32 = 1;
const MAX_HEIGHT: u32 = 256;

#[derive(Clone, Copy, Eq, PartialEq)]
enum BmpDepth {
    One,
    Four,
    Eight,
    Sixteen,
    TwentyFour,
    ThirtyTwo,
}

// ========================================================================= //

macro_rules! invalid_data {
    ($e:expr) => {
        return Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData,
                                         $e))
    };
    ($fmt:expr, $($arg:tt)+) => {
        return Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData,
                                         format!($fmt, $($arg)+)))
    };
}

macro_rules! invalid_input {
    ($e:expr) => {
        return Err(::std::io::Error::new(::std::io::ErrorKind::InvalidInput,
                                         $e))
    };
    ($fmt:expr, $($arg:tt)+) => {
        return Err(::std::io::Error::new(::std::io::ErrorKind::InvalidInput,
                                         format!($fmt, $($arg)+)))
    };
}

// ========================================================================= //

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// The type of resource stored in an ICO/CUR file.
pub enum ResourceType {
    /// Plain images (ICO files)
    Icon,
    /// Images with cursor hotspots (CUR files)
    Cursor,
}

impl ResourceType {
    pub(crate) fn from_number(number: u16) -> Option<ResourceType> {
        match number {
            1 => Some(ResourceType::Icon),
            2 => Some(ResourceType::Cursor),
            _ => None,
        }
    }

    pub(crate) fn number(&self) -> u16 {
        match *self {
            ResourceType::Icon => 1,
            ResourceType::Cursor => 2,
        }
    }
}

// ========================================================================= //

/// A collection of images; the contents of a single ICO or CUR file.
pub struct IconDir {
    restype: ResourceType,
    entries: Vec<IconDirEntry>,
}

impl IconDir {
    /// Creates a new, empty collection of icons/cursors.
    pub fn new(resource_type: ResourceType) -> IconDir {
        IconDir {
            restype: resource_type,
            entries: Vec::new(),
        }
    }

    /// Returns the type of resource stored in this collection, either icons or
    /// cursors.
    pub fn resource_type(&self) -> ResourceType { self.restype }

    /// Returns the entries in this collection.
    pub fn entries(&self) -> &[IconDirEntry] { &self.entries }

    /// Encodes an image as a new entry in this collection.  Returns an error
    /// if the encoding fails.
    pub fn add_entry(&mut self, image: IconImage) -> io::Result<()> {
        // TODO: Support setting cursor hotspots.
        let mut data = Vec::new();
        image.to_png(&mut data)?;
        let entry = IconDirEntry {
            width: image.width(),
            height: image.height(),
            num_colors: 0,
            color_planes: 0,
            bits_per_pixel: 32,
            data,
        };
        self.entries.push(entry);
        Ok(())
    }

    /// Reads an ICO or CUR file into memory.
    pub fn read<R: Read + Seek>(mut reader: R) -> io::Result<IconDir> {
        let reserved = reader.read_u16::<LittleEndian>()?;
        if reserved != 0 {
            invalid_data!("Invalid reserved field value in ICONDIR \
                           (was {}, but must be 0)",
                          reserved);
        }
        let restype = reader.read_u16::<LittleEndian>()?;
        let restype = match ResourceType::from_number(restype) {
            Some(restype) => restype,
            None => invalid_data!("Invalid resource type ({})", restype),
        };
        let num_entries = reader.read_u16::<LittleEndian>()? as usize;
        let mut entries = Vec::<IconDirEntry>::with_capacity(num_entries);
        let mut spans = Vec::<(u32, u32)>::with_capacity(num_entries);
        for _ in 0..num_entries {
            let width = reader.read_u8()?;
            let height = reader.read_u8()?;
            let num_colors = reader.read_u8()?;
            let reserved = reader.read_u8()?;
            if reserved != 0 {
                invalid_data!("Invalid reserved field value in ICONDIRENTRY \
                               (was {}, but must be 0)",
                              reserved);
            }
            let color_planes = reader.read_u16::<LittleEndian>()?;
            let bits_per_pixel = reader.read_u16::<LittleEndian>()?;
            let data_size = reader.read_u32::<LittleEndian>()?;
            let data_offset = reader.read_u32::<LittleEndian>()?;
            spans.push((data_offset, data_size));
            let entry = IconDirEntry {
                width: if width == 0 { 256 } else { width as u32 },
                height: if height == 0 { 256 } else { height as u32 },
                num_colors,
                color_planes,
                bits_per_pixel,
                data: Vec::new(),
            };
            entries.push(entry);
        }
        for (index, &(data_offset, data_size)) in spans.iter().enumerate() {
            reader.seek(SeekFrom::Start(data_offset as u64))?;
            let mut data = vec![0u8; data_size as usize];
            reader.read_exact(&mut data)?;
            entries[index].data = data;
        }
        Ok(IconDir { restype, entries })
    }

    /// Writes an ICO or CUR file out to disk.
    pub fn write<W: Write>(&self, mut writer: W) -> io::Result<()> {
        if self.entries.len() > (u16::MAX as usize) {
            invalid_input!("Too many entries in IconDir \
                            (was {}, but max is {})",
                           self.entries.len(),
                           u16::MAX);
        }
        writer.write_u16::<LittleEndian>(0)?; // reserved
        writer.write_u16::<LittleEndian>(self.restype.number())?;
        writer.write_u16::<LittleEndian>(self.entries.len() as u16)?;
        let mut data_offset = 6 + 16 * (self.entries.len() as u32);
        for entry in self.entries.iter() {
            let width = if entry.width > 255 {
                0
            } else {
                entry.width as u8
            };
            writer.write_u8(width)?;
            let height = if entry.height > 255 {
                0
            } else {
                entry.height as u8
            };
            writer.write_u8(height)?;
            writer.write_u8(entry.num_colors)?;
            writer.write_u8(0)?; // reserved
            writer.write_u16::<LittleEndian>(entry.color_planes)?;
            writer.write_u16::<LittleEndian>(entry.bits_per_pixel)?;
            let data_size = entry.data.len() as u32;
            writer.write_u32::<LittleEndian>(data_size)?;
            writer.write_u32::<LittleEndian>(data_offset)?;
            data_offset += data_size;
        }
        for entry in self.entries.iter() {
            writer.write_all(&entry.data)?;
        }
        Ok(())
    }
}

// ========================================================================= //

/// One entry in an ICO or CUR file; a single icon or cursor.
pub struct IconDirEntry {
    width: u32,
    height: u32,
    num_colors: u8,
    color_planes: u16,
    bits_per_pixel: u16,
    data: Vec<u8>,
}

impl IconDirEntry {
    /// Returns the width of the image, in pixels.
    pub fn width(&self) -> u32 { self.width }

    /// Returns the height of the image, in pixels.
    pub fn height(&self) -> u32 { self.height }

    /// Returns true if this image is encoded as a PNG, or false if it is
    /// encoded as a BMP.
    pub fn is_png(&self) -> bool { self.data.starts_with(PNG_SIGNATURE) }

    // TODO: Support getting cursor hotspots.

    /// Decodes this entry into an image.  Returns an error if the data is
    /// malformed or can't be decoded.
    pub fn decode(&self) -> io::Result<IconImage> {
        let image = if self.is_png() {
            IconImage::from_png(self.data.as_slice())?
        } else {
            IconImage::from_bmp(self.data.as_slice())?
        };
        if image.width != self.width || image.height != self.height {
            invalid_data!("Encoded image has wrong dimensions \
                           (was {}x{}, but should be {}x{})",
                          image.width,
                          image.height,
                          self.width,
                          self.height);
        }
        Ok(image)
    }
}

// ========================================================================= //

/// A decoded image.
pub struct IconImage {
    width: u32,
    height: u32,
    rgba_data: Vec<u8>,
}

impl IconImage {
    /// Creates a new image with the given dimensions and RGBA data.  The
    /// `width` and `height` must each be between 1 and 256 inclusive, and
    /// `rgba_data` must have `4 * width * height` bytes and be in row-major
    /// order from top to bottom.  Panics if the dimensions are out of range or
    /// if `rgba_data` is the wrong length.
    pub fn from_rgba_data(width: u32, height: u32, rgba_data: Vec<u8>)
                          -> IconImage {
        if width < MIN_WIDTH || width > MAX_WIDTH {
            panic!("Invalid width (was {}, but range is {}-{})",
                   width,
                   MIN_WIDTH,
                   MAX_WIDTH);
        }
        if height < MIN_HEIGHT || height > MAX_HEIGHT {
            panic!("Invalid height (was {}, but range is {}-{})",
                   height,
                   MIN_HEIGHT,
                   MAX_HEIGHT);
        }
        let expected_data_len = (4 * width * height) as usize;
        if rgba_data.len() != expected_data_len {
            panic!("Invalid data length \
                    (was {}, but must be {} for {}x{} image)",
                   rgba_data.len(),
                   expected_data_len,
                   width,
                   height);
        }
        IconImage {
            width,
            height,
            rgba_data,
        }
    }

    /// Decodes an image from a PNG file.  The width and height of the image
    /// must each be between 1 and 256 inclusive.  Returns an error if the PNG
    /// data is malformed or can't be decoded, or if the size of the PNG image
    /// is out of range.
    pub fn from_png<R: Read>(reader: R) -> io::Result<IconImage> {
        let decoder = png::Decoder::new(reader);
        let (info, mut reader) = match decoder.read_info() {
            Ok(tuple) => tuple,
            Err(error) => invalid_data!("Malformed PNG data; {}", error),
        };
        if info.width < MIN_WIDTH || info.width > MAX_WIDTH {
            invalid_data!("Invalid PNG width (was {}, but range is {}-{})",
                          info.width,
                          MIN_WIDTH,
                          MAX_WIDTH);
        }
        if info.height < MIN_HEIGHT || info.height > MAX_HEIGHT {
            invalid_data!("Invalid PNG height (was {}, but range is {}-{})",
                          info.height,
                          MIN_HEIGHT,
                          MAX_HEIGHT);
        }
        if info.bit_depth != png::BitDepth::Eight {
            // TODO: Support other bit depths.
            invalid_data!("Unsupported PNG bit depth: {:?}", info.bit_depth);
        }
        let mut buffer = vec![0u8; info.buffer_size()];
        match reader.next_frame(&mut buffer) {
            Ok(()) => {}
            Err(error) => invalid_data!("Malformed PNG data; {}", error),
        }
        let rgba_data = match info.color_type {
            png::ColorType::RGBA => buffer,
            png::ColorType::RGB => {
                let num_pixels = buffer.len() / 3;
                let mut rgba = Vec::with_capacity(num_pixels * 4);
                for i in 0..num_pixels {
                    rgba.extend_from_slice(&buffer[(3 * i)..][..3]);
                    rgba.push(u8::MAX);
                }
                rgba
            }
            png::ColorType::GrayscaleAlpha => {
                let num_pixels = buffer.len() / 2;
                let mut rgba = Vec::with_capacity(num_pixels * 4);
                for i in 0..num_pixels {
                    let gray = buffer[2 * i];
                    let alpha = buffer[2 * i + 1];
                    rgba.push(gray);
                    rgba.push(gray);
                    rgba.push(gray);
                    rgba.push(alpha);
                }
                rgba
            }
            png::ColorType::Grayscale => {
                let mut rgba = Vec::with_capacity(buffer.len() * 4);
                for value in buffer.into_iter() {
                    rgba.push(value);
                    rgba.push(value);
                    rgba.push(value);
                    rgba.push(std::u8::MAX);
                }
                rgba
            }
            png::ColorType::Indexed => {
                // TODO: Implement ColorType::Indexed conversion
                invalid_data!("Unsupported PNG color type: {:?}",
                              info.color_type);
            }
        };
        Ok(IconImage::from_rgba_data(info.width, info.height, rgba_data))
    }

    pub(crate) fn from_bmp<R: Read>(mut reader: R) -> io::Result<IconImage> {
        // Read the BITMAPINFOHEADER struct:
        let data_size = reader.read_u32::<LittleEndian>()?;
        if data_size != BMP_HEADER_LEN {
            invalid_data!("Invalid BMP header size (was {}, must be {})",
                          data_size,
                          BMP_HEADER_LEN);
        }
        let width = reader.read_i32::<LittleEndian>()?;
        if width < (MIN_WIDTH as i32) || width > (MAX_WIDTH as i32) {
            invalid_data!("Invalid BMP width (was {}, but range is {}-{})",
                          width,
                          MIN_WIDTH,
                          MAX_WIDTH);
        }
        let width = width as u32;
        let height = reader.read_i32::<LittleEndian>()?;
        if height % 2 != 0 {
            // The height is stored doubled, counting the rows of both the
            // color data and the alpha mask, so it should be divisible by 2.
            invalid_data!("Invalid height field in BMP header \
                           (was {}, but must be divisible by 2)",
                          height);
        }
        let height = height / 2;
        if height < (MIN_HEIGHT as i32) || height > (MAX_HEIGHT as i32) {
            invalid_data!("Invalid BMP height (was {}, but range is {}-{})",
                          height,
                          MIN_HEIGHT,
                          MAX_HEIGHT);
        }
        let height = height as u32;
        let _planes = reader.read_u16::<LittleEndian>()?;
        let bits_per_pixel = reader.read_u16::<LittleEndian>()? as u32;
        let _compression = reader.read_u32::<LittleEndian>()?;
        let _image_size = reader.read_u32::<LittleEndian>()?;
        let _horz_ppm = reader.read_i32::<LittleEndian>()?;
        let _vert_ppm = reader.read_i32::<LittleEndian>()?;
        let _colors_used = reader.read_u32::<LittleEndian>()?;
        let _colors_important = reader.read_u32::<LittleEndian>()?;

        // Determine the size of the color table:
        let (depth, num_colors) = match bits_per_pixel {
            1 => (BmpDepth::One, 2),
            4 => (BmpDepth::Four, 16),
            8 => (BmpDepth::Eight, 256),
            16 => (BmpDepth::Sixteen, 0),
            24 => (BmpDepth::TwentyFour, 0),
            32 => (BmpDepth::ThirtyTwo, 0),
            _ => {
                invalid_data!("Unsupported BMP bits-per-pixel ({})",
                              bits_per_pixel);
            }
        };

        // Read in the color table:
        let mut color_table = Vec::<(u8, u8, u8)>::with_capacity(num_colors);
        for _ in 0..num_colors {
            let blue = reader.read_u8()?;
            let green = reader.read_u8()?;
            let red = reader.read_u8()?;
            let _reserved = reader.read_u8()?;
            color_table.push((red, green, blue));
        }

        // Read in the color data, which is stored row by row, starting from
        // the *bottom* row:
        let num_pixels = (width * height) as usize;
        let mut rgba = vec![u8::MAX; num_pixels * 4];
        let row_data_size = (width * bits_per_pixel + 7) / 8;
        let row_padding_size = ((row_data_size + 3) / 4) * 4 - row_data_size;
        let mut row_padding = vec![0; row_padding_size as usize];
        for row in 0..height {
            let mut start = (4 * (height - row - 1) * width) as usize;
            match depth {
                BmpDepth::One => {
                    let mut col = 0;
                    for _ in 0..row_data_size {
                        let byte = reader.read_u8()?;
                        for bit in 0..8 {
                            let index = (byte >> (7 - bit)) & 0x1;
                            let (red, green, blue) = color_table[index as
                                                                     usize];
                            rgba[start] = red;
                            rgba[start + 1] = green;
                            rgba[start + 2] = blue;
                            col += 1;
                            if col == width {
                                break;
                            }
                            start += 4;
                        }
                    }
                }
                BmpDepth::Four => {
                    let mut col = 0;
                    for _ in 0..row_data_size {
                        let byte = reader.read_u8()?;
                        for nibble in 0..2 {
                            let index = (byte >> (4 * (1 - nibble))) & 0xf;
                            let (red, green, blue) = color_table[index as
                                                                     usize];
                            rgba[start] = red;
                            rgba[start + 1] = green;
                            rgba[start + 2] = blue;
                            col += 1;
                            if col == width {
                                break;
                            }
                            start += 4;
                        }
                    }
                }
                BmpDepth::Eight => {
                    for _ in 0..width {
                        let index = reader.read_u8()?;
                        let (red, green, blue) = color_table[index as usize];
                        rgba[start] = red;
                        rgba[start + 1] = green;
                        rgba[start + 2] = blue;
                        start += 4;
                    }
                }
                BmpDepth::Sixteen => {
                    for _ in 0..width {
                        let color = reader.read_u16::<LittleEndian>()?;
                        let red = (color >> 10) & 0x1f;
                        let green = (color >> 5) & 0x1f;
                        let blue = color & 0x1f;
                        rgba[start] = ((red * 255 + 15) / 31) as u8;
                        rgba[start + 1] = ((green * 255 + 15) / 31) as u8;
                        rgba[start + 2] = ((blue * 255 + 15) / 31) as u8;
                        start += 4;
                    }
                }
                BmpDepth::TwentyFour => {
                    for _ in 0..width {
                        let blue = reader.read_u8()?;
                        let green = reader.read_u8()?;
                        let red = reader.read_u8()?;
                        rgba[start] = red;
                        rgba[start + 1] = green;
                        rgba[start + 2] = blue;
                        start += 4;
                    }
                }
                BmpDepth::ThirtyTwo => {
                    for _ in 0..width {
                        let blue = reader.read_u8()?;
                        let green = reader.read_u8()?;
                        let red = reader.read_u8()?;
                        let alpha = reader.read_u8()?;
                        rgba[start] = red;
                        rgba[start + 1] = green;
                        rgba[start + 2] = blue;
                        rgba[start + 3] = alpha;
                        start += 4;
                    }
                }
            }
            reader.read_exact(&mut row_padding)?;
        }

        // Read in the alpha mask (1 bit per pixel), which again is stored row
        // by row, starting from the *bottom* row, with each row padded to a
        // multiple of four bytes:
        if depth != BmpDepth::ThirtyTwo {
            let row_mask_size = (width + 7) / 8;
            let row_padding_size = ((row_mask_size + 3) / 4) * 4 -
                row_mask_size;
            let mut row_padding = vec![0; row_padding_size as usize];
            for row in 0..height {
                let mut start = (4 * (height - row - 1) * width) as usize;
                let mut col = 0;
                for _ in 0..row_mask_size {
                    let byte = reader.read_u8()?;
                    for bit in 0..8 {
                        if ((byte >> (7 - bit)) & 0x1) == 1 {
                            rgba[start + 3] = 0;
                        }
                        start += 4;
                        col += 1;
                        if col == width {
                            break;
                        }
                    }
                }
                reader.read_exact(&mut row_padding)?;
            }
        }

        Ok(IconImage::from_rgba_data(width, height, rgba))
    }

    /// Encodes the image as a PNG file.
    pub fn to_png<W: Write>(&self, writer: W) -> io::Result<()> {
        let mut encoder = png::Encoder::new(writer, self.width, self.height);
        // TODO: Detect if we can encode the image more efficiently.
        encoder.set(png::ColorType::RGBA).set(png::BitDepth::Eight);
        let result =
            encoder.write_header().and_then(|mut writer| {
                writer.write_image_data(&self.rgba_data)
            });
        match result {
            Ok(()) => Ok(()),
            Err(png::EncodingError::IoError(error)) => return Err(error),
            Err(png::EncodingError::Format(error)) => {
                invalid_input!("PNG format error: {}", error);
            }
        }
    }

    /// Returns the width of the image, in pixels.
    pub fn width(&self) -> u32 { self.width }

    /// Returns the height of the image, in pixels.
    pub fn height(&self) -> u32 { self.height }

    /// Returns the RGBA data for this image, in row-major order from top to
    /// bottom.
    pub fn rgba_data(&self) -> &[u8] { &self.rgba_data }
}

// ========================================================================= //

#[cfg(test)]
mod tests {
    use super::{IconDir, IconImage, ResourceType};
    use std::io::Cursor;

    #[test]
    fn resource_type_round_trip() {
        let restypes = &[ResourceType::Icon, ResourceType::Cursor];
        for &restype in restypes.iter() {
            assert_eq!(ResourceType::from_number(restype.number()),
                       Some(restype));
        }
    }

    #[test]
    fn read_empty_icon_set() {
        let input = b"\x00\x00\x01\x00\x00\x00";
        let icondir = IconDir::read(Cursor::new(input)).unwrap();
        assert_eq!(icondir.resource_type(), ResourceType::Icon);
        assert_eq!(icondir.entries().len(), 0);
    }

    #[test]
    fn read_empty_cursor_set() {
        let input = b"\x00\x00\x02\x00\x00\x00";
        let icondir = IconDir::read(Cursor::new(input)).unwrap();
        assert_eq!(icondir.resource_type(), ResourceType::Cursor);
        assert_eq!(icondir.entries().len(), 0);
    }

    #[test]
    fn write_empty_icon_set() {
        let icondir = IconDir::new(ResourceType::Icon);
        let mut output = Vec::<u8>::new();
        icondir.write(&mut output).unwrap();
        let expected: &[u8] = b"\x00\x00\x01\x00\x00\x00";
        assert_eq!(output.as_slice(), expected);
    }

    #[test]
    fn write_empty_cursor_set() {
        let icondir = IconDir::new(ResourceType::Cursor);
        let mut output = Vec::<u8>::new();
        icondir.write(&mut output).unwrap();
        let expected: &[u8] = b"\x00\x00\x02\x00\x00\x00";
        assert_eq!(output.as_slice(), expected);
    }

    #[test]
    fn read_bmp_1bpp_icon() {
        let input: &[u8] = b"\
            \x00\x00\x01\x00\x01\x00\
            \
            \x02\x02\x02\x00\x01\x00\x01\x00\
            \x40\x00\x00\x00\x16\x00\x00\x00\
            \
            \x28\x00\x00\x00\x02\x00\x00\x00\x04\x00\x00\x00\
            \x01\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x00\x00\
            \
            \x55\x00\x55\x00\xff\xff\xff\x00\
            \
            \xc0\x00\x00\x00\
            \x40\x00\x00\x00\
            \
            \x40\x00\x00\x00\
            \x00\x00\x00\x00";
        let icondir = IconDir::read(Cursor::new(input)).unwrap();
        assert_eq!(icondir.resource_type(), ResourceType::Icon);
        assert_eq!(icondir.entries().len(), 1);
        let entry = &icondir.entries()[0];
        assert_eq!(entry.width(), 2);
        assert_eq!(entry.height(), 2);
        assert!(!entry.is_png());
        let image = entry.decode().unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        let rgba: &[u8] = b"\
            \x55\x00\x55\xff\xff\xff\xff\xff\
            \xff\xff\xff\xff\xff\xff\xff\x00";
        assert_eq!(image.rgba_data(), rgba);
    }

    #[test]
    fn read_bmp_4bpp_icon() {
        let input: &[u8] = b"\
            \x00\x00\x01\x00\x01\x00\
            \
            \x05\x03\x10\x00\x01\x00\x04\x00\
            \x80\x00\x00\x00\x16\x00\x00\x00\
            \
            \x28\x00\x00\x00\x05\x00\x00\x00\x06\x00\x00\x00\
            \x01\x00\x04\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x00\x00\
            \
            \x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x7f\x00\x00\x00\xff\x00\
            \x00\x7f\x00\x00\x00\xff\x00\x00\
            \x00\x7f\x7f\x00\x00\xff\xff\x00\
            \x7f\x00\x00\x00\xff\x00\x00\x00\
            \x7f\x00\x7f\x00\xff\x00\xff\x00\
            \x7f\x7f\x00\x00\xff\xff\x00\x00\
            \x7f\x7f\x7f\x00\xff\xff\xff\x00\
            \
            \x0f\x35\x00\x00\
            \xf3\x59\x10\x00\
            \x05\x91\x00\x00\
            \
            \x88\x00\x00\x00\
            \x00\x00\x00\x00\
            \x88\x00\x00\x00";
        let icondir = IconDir::read(Cursor::new(input)).unwrap();
        assert_eq!(icondir.resource_type(), ResourceType::Icon);
        assert_eq!(icondir.entries().len(), 1);
        let entry = &icondir.entries()[0];
        assert_eq!(entry.width(), 5);
        assert_eq!(entry.height(), 3);
        assert!(!entry.is_png());
        let image = entry.decode().unwrap();
        assert_eq!(image.width(), 5);
        assert_eq!(image.height(), 3);
        let rgba: &[u8] = b"\
            \x00\x00\x00\x00\x00\xff\x00\xff\x00\x00\xff\xff\
            \x00\x00\x00\xff\x00\x00\x00\x00\
            \xff\xff\xff\xff\xff\x00\x00\xff\x00\xff\x00\xff\
            \x00\x00\xff\xff\x00\x00\x00\xff\
            \x00\x00\x00\x00\xff\xff\xff\xff\xff\x00\x00\xff\
            \x00\xff\x00\xff\x00\x00\x00\x00";
        assert_eq!(image.rgba_data(), rgba);
    }

    #[test]
    fn read_png_grayscale_icon() {
        let input: &[u8] = b"\
            \x00\x00\x01\x00\x01\x00\
            \
            \x02\x02\x00\x00\x00\x00\x00\x00\
            \x47\x00\x00\x00\x16\x00\x00\x00\
            \
            \x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\
            \x00\x00\x00\x02\x00\x00\x00\x02\x08\x00\x00\x00\x00\x57\xdd\x52\
            \xf8\x00\x00\x00\x0e\x49\x44\x41\x54\x78\x9c\x63\xb4\x77\x60\xdc\
            \xef\x00\x00\x04\x08\x01\x81\x86\x2e\xc9\x8d\x00\x00\x00\x00\x49\
            \x45\x4e\x44\xae\x42\x60\x82";
        let icondir = IconDir::read(Cursor::new(input)).unwrap();
        assert_eq!(icondir.resource_type(), ResourceType::Icon);
        assert_eq!(icondir.entries().len(), 1);
        let entry = &icondir.entries()[0];
        assert_eq!(entry.width(), 2);
        assert_eq!(entry.height(), 2);
        assert!(entry.is_png());
        let image = entry.decode().unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        let rgba: &[u8] = b"\
            \x3f\x3f\x3f\xff\x7f\x7f\x7f\xff\
            \xbf\xbf\xbf\xff\xff\xff\xff\xff";
        assert_eq!(image.rgba_data(), rgba);
    }

    #[test]
    fn image_data_round_trip() {
        // Create an image:
        let width = 11;
        let height = 13;
        let mut rgba = Vec::new();
        for index in 0..(width * height) {
            rgba.push(if index % 2 == 0 { 0 } else { 255 });
            rgba.push(if index % 3 == 0 { 0 } else { 255 });
            rgba.push(if index % 5 == 0 { 0 } else { 255 });
            rgba.push(if index % 7 == 0 { 128 } else { 255 });
        }
        let image = IconImage::from_rgba_data(width, height, rgba.clone());
        // Write that image into an ICO file:
        let mut icondir = IconDir::new(ResourceType::Icon);
        icondir.add_entry(image).unwrap();
        let mut file = Vec::<u8>::new();
        icondir.write(&mut file).unwrap();
        // Read the ICO file back in and make sure the image is the same:
        let icondir = IconDir::read(Cursor::new(&file)).unwrap();
        assert_eq!(icondir.entries().len(), 1);
        let image = icondir.entries()[0].decode().unwrap();
        assert_eq!(image.width(), width);
        assert_eq!(image.height(), height);
        assert_eq!(image.rgba_data(), rgba.as_slice());
    }
}

// ========================================================================= //
