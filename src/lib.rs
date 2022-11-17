use std::io::Write;
use std::{io, mem, slice};
use byteorder::{LittleEndian, WriteBytesExt};

const SIGNATURE: [u8; SIGNATURE_SIZE] = *b"TRUEVISION-XFILE";
const SIGNATURE_SIZE: usize = 16;
const HEADER_SIZE: usize = 18;
const FOOTER_SIZE: usize = 26;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct BitDepth(u8);

impl Default for BitDepth {
    fn default() -> Self {
        BitDepth::B32
    }
}

impl BitDepth {
    const B8: BitDepth = BitDepth(8);
    const B32: BitDepth = BitDepth(32);
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ColorMapType(u8);

impl Default for ColorMapType {
    fn default() -> Self {
        ColorMapType::ABSENT
    }
}

impl ColorMapType {
    const ABSENT: ColorMapType = ColorMapType(0);
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ImageType(u8);

impl Default for ImageType {
    fn default() -> Self {
        ImageType::TRUE_COLOR
    }
}

impl ImageType {
    const TRUE_COLOR: ImageType = ImageType(2);
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum HorizontalOrdering {
    LeftToRight,
    RightToLeft,
}

impl Default for HorizontalOrdering {
    fn default() -> Self {
        HorizontalOrdering::LeftToRight
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum VerticalOrdering {
    BottomToTop,
    TopToBottom,
}

impl Default for VerticalOrdering {
    fn default() -> Self {
        VerticalOrdering::BottomToTop
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct ImageDescriptor(u8);

#[derive(Copy, Clone, Debug, Default)]
struct ImageDescriptorBuilder {
    alpha_depth: BitDepth,
    horizontal_ordering: HorizontalOrdering,
    vertical_ordering: VerticalOrdering,
}

impl ImageDescriptorBuilder {
    const HORIZONTAL_ORDERING_BITMASK: u8 = 0b00010000;
    const VERTICAL_ORDERING_BITMASK: u8 = 0b00100000;

    fn new() -> Self {
        ImageDescriptorBuilder::default()
    }

    fn build(&self) -> ImageDescriptor {
        let mut value = 0;

        let BitDepth(alpha_depth) = self.alpha_depth;
        value |= alpha_depth;

        if self.horizontal_ordering == HorizontalOrdering::RightToLeft {
            value |= ImageDescriptorBuilder::HORIZONTAL_ORDERING_BITMASK;
        }

        if self.vertical_ordering == VerticalOrdering::TopToBottom {
            value |= ImageDescriptorBuilder::VERTICAL_ORDERING_BITMASK;
        }

        ImageDescriptor(value)
    }

    fn with_alpha(&mut self, depth: BitDepth) -> &mut Self {
        self.alpha_depth = depth;

        self
    }

    fn with_horizontal_ordering(&mut self, ordering: HorizontalOrdering) -> &mut Self {
        self.horizontal_ordering = ordering;

        self
    }

    fn with_vertical_ordering(&mut self, ordering: VerticalOrdering) -> &mut Self {
        self.vertical_ordering = ordering;

        self
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct ColorMapSpecification {
    pub first_entry_index: u16,
    pub entry_count: u16,
    pub color_depth: BitDepth,
}

impl ColorMapSpecification {
    fn write_to<T: Write>(&self, w: &mut T) -> io::Result<()> {
        w.write_u16::<LittleEndian>(self.first_entry_index)?;
        w.write_u16::<LittleEndian>(self.entry_count)?;
        w.write_u8(self.color_depth.0)?;

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct ImageSpecification {
    pub x_origin: u16,
    pub y_origin: u16,
    pub width: u16,
    pub height: u16,
    pub pixel_depth: BitDepth,
    pub descriptor: ImageDescriptor,
}

impl ImageSpecification {
    fn write_to<T: Write>(&self, w: &mut T) -> io::Result<()> {
        w.write_u16::<LittleEndian>(self.x_origin)?;
        w.write_u16::<LittleEndian>(self.y_origin)?;
        w.write_u16::<LittleEndian>(self.width)?;
        w.write_u16::<LittleEndian>(self.height)?;
        w.write_u8(self.pixel_depth.0)?;
        w.write_u8(self.descriptor.0)?;

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
struct Header {
    id_length: u8,
    color_map_type: ColorMapType,
    image_type: ImageType,
    color_map_specification: ColorMapSpecification,
    image_specification: ImageSpecification,
}

impl Header {
    fn write_to<T: Write>(&self, w: &mut T) -> io::Result<()> {
        w.write_u8(self.id_length)?;
        w.write_u8(self.color_map_type.0)?;
        w.write_u8(self.image_type.0)?;
        self.color_map_specification.write_to(w)?;
        self.image_specification.write_to(w)?;

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Footer {
    extension_offset: u32,
    developer_offset: u32,
    signature: [u8; SIGNATURE_SIZE],
    dot: u8,
    nul: u8,
}

impl Default for Footer {
    fn default() -> Self {
        Footer {
            extension_offset: 0,
            developer_offset: 0,
            signature: SIGNATURE,
            dot: b'.',
            nul: b'\0',
        }
    }
}

impl Footer {
    fn write_to<T: Write>(&self, w: &mut T) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.extension_offset)?;
        w.write_u32::<LittleEndian>(self.developer_offset)?;
        w.write_all(&self.signature)?;
        w.write_u8(self.dot)?;
        w.write_u8(self.nul)?;

        Ok(())
    }
}

/// A 32-bit uncompressed true-color Truevision TGA file.
#[derive(Clone, Debug, Default)]
pub struct Image {
    data: Vec<u8>,
    width: u16,
    height: u16,
}

impl Image {
    /// Calculates the size in bytes of an image with the given dimensions.
    pub fn effective_size(width: u16, height: u16) -> usize {
        width as usize * BitDepth::B32.0 as usize / 8 * height as usize
    }

    pub fn new(width: u16, height: u16, data: Vec<u8>) -> Self {
        Image {
            data,
            width,
            height,
        }
    }

    pub fn write_to<T: Write>(&self, w: &mut T) -> io::Result<()> {
        let header = Header {
            image_specification: ImageSpecification {
                width: self.width,
                height: self.height,
                descriptor: ImageDescriptorBuilder::new()
                    .with_alpha(BitDepth::B8)
                    .with_vertical_ordering(VerticalOrdering::TopToBottom)
                    .build(),
                ..Default::default()
            },
            ..Default::default()
        };

        let footer = Footer::default();

        header.write_to(w)?;
        w.write_all(&self.data)?;
        footer.write_to(w)?;

        Ok(())
    }
}
