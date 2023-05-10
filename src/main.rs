use std::fs;
use std::str;

#[derive(Debug)]
struct OggSegment {
    size: u8,
    data: Vec<u8>,
}

impl From<&[u8]> for OggSegment {
    fn from(value: &[u8]) -> Self {
        Self {
            size: value.len().try_into().unwrap(),
            data: value.to_owned(),
        }
    }
}

#[derive(Debug)]
struct OggPage {
    signature: String,
    version: u8,
    flags: u8,
    granule_position: u64,
    serial_number: u32,
    sequence_number: u32,
    checksum: u32,
    total_segments: u8,
    segments: Vec<OggSegment>,
}

#[derive(Debug)]
struct OggParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> OggParser<'a> {
    fn new(data: &'a [u8]) -> OggParser<'a> {
        Self {
            data,
            pos: 0,
        }
    }
}

impl Iterator for OggParser<'_> {
    type Item = OggPage;

    fn next(&mut self) -> Option<Self::Item> {
        let OggParser { data, pos } = self;
        let ps = *pos;

        if ps >= data.len() {
            return None;
        }
        
        // let signature = u32::from_le_bytes(data[ps..ps + 4].try_into().unwrap());
        let signature = str::from_utf8(&data[ps..ps + 4]).unwrap().to_owned();
        let version = u8::from_le_bytes(data[ps + 4..ps + 5].try_into().unwrap());
        let flags = u8::from_le_bytes(data[ps + 5..ps + 6].try_into().unwrap());
        let granule_position = u64::from_le_bytes(data[ps + 6..ps + 14].try_into().unwrap());
        let serial_number = u32::from_le_bytes(data[ps + 14..ps + 18].try_into().unwrap());
        let sequence_number = u32::from_le_bytes(data[ps + 18..ps + 22].try_into().unwrap());
        let checksum = u32::from_le_bytes(data[ps + 22..ps + 26].try_into().unwrap());
        let total_segments = u8::from_le_bytes(data[ps + 26..ps + 27].try_into().unwrap());
        
        let segment_sizes = &data[ps + 27..ps + 27 + total_segments as usize];

        let seg_start = ps + 27 + total_segments as usize;

        let segments: Vec<OggSegment> = segment_sizes
            .iter()
            .scan((seg_start, 0), |(seg_start, size), &x| {
                Some((*seg_start + *size, x))
            })
            .map(|(seg_start, size)| {
                data[seg_start..seg_start + size as usize].try_into().unwrap()
            }).collect();

        let page_segments_len = segment_sizes.iter().fold(0usize, |a, &b| a + b as usize);
        self.pos = seg_start + page_segments_len;

        Some(OggPage {
            signature,
            version,
            flags,
            granule_position,
            serial_number,
            sequence_number,
            checksum,
            total_segments,
            segments,
        })
    }
}

fn main() {
    let data = fs::read("/home/winter/Downloads/mysunset.opus")
        .expect("Could not read audio file");

    let pages: Vec<OggPage> = OggParser::new(&data).into_iter().collect();

    println!("{:?}", pages[20]);
}
