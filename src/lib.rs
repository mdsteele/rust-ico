//! A library for encoding/decoding ICO images files.

#![warn(missing_docs)]

extern crate byteorder;
extern crate png;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use png::HasParameters;
use std::{u16, u8};
use std::io::{self, Read, Seek, SeekFrom, Write};

// ========================================================================= //

// The signature that all PNG files start with.
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G'];

const MIN_WIDTH: u32 = 1;
const MAX_WIDTH: u32 = 256;
const MIN_HEIGHT: u32 = 1;
const MAX_HEIGHT: u32 = 256;

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
/// The type of resource stored in an ICO file.
pub enum ResourceType {
    /// Plain images
    Icon,
    /// Images with cursor hotspots
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

/// A collection of images; the contents of a single ICO file.
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

    /// Reads an ICO file into memory.
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

    /// Writes an ICO file out to disk.
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

/// One entry in an ICO file; a single icon or cursor.
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

    /// Decodes this entry into an image.  Returns an error if the data is
    /// malformed or can't be decoded.
    pub fn decode(&self) -> io::Result<IconImage> {
        if self.data.starts_with(PNG_SIGNATURE) {
            let image = IconImage::from_png(self.data.as_slice())?;
            if image.width != self.width || image.height != self.height {
                invalid_data!("Encoded PNG has wrong dimensions \
                               (was {}x{}, but should be {}x{})",
                              image.width,
                              image.height,
                              self.width,
                              self.height);
            }
            Ok(image)
        } else {
            // TODO: Implement BMP decoding
            invalid_data!("Decoding non-PNG images is not yet supported");
        }
    }
}

// ========================================================================= //

/// A decoded image from an ICO file.
pub struct IconImage {
    width: u32,
    height: u32,
    rgba_data: Vec<u8>,
}

impl IconImage {
    /// Creates a new image with the given dimensions and RGBA data.  The
    /// `width` and `height` must each be between 1 and 256 inclusive, and
    /// `rgba_data` must have `4 * width * height` bytes and be in row-major
    /// order.  Panics if the dimensions are out of range or if `rgba_data` is
    /// the wrong length.
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
            invalid_data!("Invalid width (was {}, but range is {}-{})",
                          info.width,
                          MIN_WIDTH,
                          MAX_WIDTH);
        }
        if info.height < MIN_HEIGHT || info.height > MAX_HEIGHT {
            invalid_data!("Invalid height (was {}, but range is {}-{})",
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

    /// Returns the RGBA data for this image, in row-major order.
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
