//! A library for encoding/decoding ICO images files.

#![warn(missing_docs)]

extern crate byteorder;
extern crate png;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::u16;

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

/// One entry in an ICO file; a single image or cursor.
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
}

// ========================================================================= //

#[cfg(test)]
mod tests {
    use super::{IconDir, ResourceType};
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
}

// ========================================================================= //
