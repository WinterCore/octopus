use std::str;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

#[derive(Debug, Clone, Default)]
pub struct OpusComments {
    pub vendor: String,
    pub comments: HashMap<String, String>,
}

impl OpusComments {
    pub fn title(&self) -> Option<&str> {
        self.comments.get("TITLE").map(|s| s.as_str())
    }

    pub fn artist(&self) -> Option<&str> {
        self.comments.get("ARTIST").map(|s| s.as_str())
    }

    pub fn album(&self) -> Option<&str> {
        self.comments.get("ALBUM").map(|s| s.as_str())
    }

    pub fn date(&self) -> Option<&str> {
        self.comments.get("DATE").map(|s| s.as_str())
    }

    pub fn genre(&self) -> Option<&str> {
        self.comments.get("GENRE").map(|s| s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct OggSegment {
    pub size: u8,
    pub data: Vec<u8>,
}

impl From<&[u8]> for OggSegment {
    fn from(value: &[u8]) -> Self {
        Self {
            size: value.len().try_into().unwrap(),
            data: value.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OggPage {
    pub signature: String,
    pub version: u8,
    pub flags: u8,
    pub granule_position: u64,
    pub serial_number: u32,
    pub sequence_number: u32,
    pub checksum: u32,
    pub total_segments: u8,
    pub segments: Vec<OggSegment>,
}

impl OggPage {
    pub fn serialize(&self) -> Vec<u8> {
        let mut data: Vec<u8> = vec![];
        data.extend(self.signature.as_bytes().iter());
        data.push(self.version);
        data.push(self.flags);
        data.extend(self.granule_position.to_le_bytes());
        data.extend(self.serial_number.to_le_bytes());
        data.extend(self.sequence_number.to_le_bytes());
        data.extend(self.checksum.to_le_bytes());
        data.push(self.total_segments);

        let segment_sizes: Vec<u8> = self.segments.iter().map(|s| s.size).collect();
        data.extend(segment_sizes);

        let segment_data: Vec<u8> = self.segments.iter().flat_map(|s| s.data.clone()).collect();
        data.extend(segment_data);

        data
    }
}

#[derive(Debug)]
pub struct OggParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> OggParser<'a> {
    pub fn new(data: &'a [u8]) -> OggParser<'a> {
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
            .scan(seg_start, |state, &x| {
                let seg_start = *state;

                *state += x as usize;

                Some((seg_start, x))
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

fn parse_opus_comment_header(data: &[u8]) -> Result<OpusComments, String> {
    let mut pos = 0;

    // Skip "OpusTags" magic signature (8 bytes)
    if data.len() < 8 || &data[0..8] != b"OpusTags" {
        return Err("Invalid Opus comment header".to_string());
    }
    pos += 8;

    // Read vendor string length (4 bytes, little-endian)
    if data.len() < pos + 4 {
        return Err("Invalid vendor string length".to_string());
    }
    let vendor_length = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;

    // Read vendor string
    if data.len() < pos + vendor_length {
        return Err("Invalid vendor string".to_string());
    }
    let vendor = String::from_utf8_lossy(&data[pos..pos + vendor_length]).to_string();
    pos += vendor_length;

    // Read user comment list length (4 bytes, little-endian)
    if data.len() < pos + 4 {
        return Err("Invalid comment list length".to_string());
    }
    let comment_count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
    pos += 4;

    let mut comments = HashMap::new();

    // Read each comment
    for _ in 0..comment_count {
        if data.len() < pos + 4 {
            return Err("Invalid comment length".to_string());
        }
        let comment_length = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if data.len() < pos + comment_length {
            return Err("Invalid comment data".to_string());
        }
        let comment_str = String::from_utf8_lossy(&data[pos..pos + comment_length]).to_string();
        pos += comment_length;

        // Parse "KEY=VALUE" format
        if let Some(equals_pos) = comment_str.find('=') {
            let key = comment_str[..equals_pos].to_uppercase();
            let value = comment_str[equals_pos + 1..].to_string();
            comments.insert(key, value);
        }
    }

    Ok(OpusComments { vendor, comments })
}

pub fn get_opus_comments(file: &mut File) -> Result<OpusComments, String> {
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| format!("Failed to read file: {}", e))?;

    let mut parser = OggParser::new(&buffer);

    // Skip the first page (Opus ID header)
    parser.next().ok_or("No Ogg pages found")?;

    // Read the comment header, which may span multiple pages
    let mut comment_data = Vec::new();
    let mut packet_complete = false;

    while !packet_complete {
        let page = parser.next().ok_or("Incomplete comment header")?;

        for segment in &page.segments {
            comment_data.extend_from_slice(&segment.data);

            // If segment size is less than 255, the packet is complete
            if segment.size < 255 {
                packet_complete = true;
                break;
            }
        }
    }

    parse_opus_comment_header(&comment_data)
}
