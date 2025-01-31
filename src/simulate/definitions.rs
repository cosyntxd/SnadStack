use core::panic;
use std::fmt;

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};
pub struct SectionInfo<'a> {
    pub text: &'a str,
    pub section: usize,
    pub line: usize,
}

#[derive(Clone)]
pub struct RgbColor {
    r: u8,
    g: u8,
    b: u8,
}
impl RgbColor {
    pub fn as_array(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
    pub fn from_array(rgb: [u8; 3]) -> Self {
        Self {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
        }
    }
    pub fn from_hex(val: SectionInfo) -> Result<Self, SectionDecodeError> {
        if val.text.len() != 7 || !val.text.starts_with("#") {
            return Err(SectionDecodeError::Hex {
                section: val.section,
            });
        }
        let r = u8::from_str_radix(&val.text[1..3], 16).map_err(|_| SectionDecodeError::Hex {
            section: val.section,
        })?;
        let g = u8::from_str_radix(&val.text[3..5], 16).map_err(|_| SectionDecodeError::Hex {
            section: val.section,
        })?;
        let b = u8::from_str_radix(&val.text[5..7], 16).map_err(|_| SectionDecodeError::Hex {
            section: val.section,
        })?;
        Ok(Self { r, g, b })
    }
    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
    pub fn lerp_fast(&self, other: &RgbColor, t: i16) -> RgbColor {
        RgbColor {
            r: (self.r as i16 + ((other.r as i16 - self.r as i16) * t) / 128) as u8,
            g: (self.g as i16 + ((other.g as i16 - self.g as i16) * t) / 128) as u8,
            b: (self.b as i16 + ((other.b as i16 - self.b as i16) * t) / 128) as u8,
        }
    }
    pub fn lerp_f32(&self, other: &RgbColor, t: f32) -> RgbColor {
        RgbColor {
            r: (self.r as f32 + t * (other.r as f32 - self.r as f32)) as u8,
            g: (self.g as f32 + t * (other.g as f32 - self.g as f32)) as u8,
            b: (self.b as f32 + t * (other.b as f32 - self.b as f32)) as u8,
        }
    }
}
enum Boolean {
    True,
    False,
}
impl Boolean {
    pub fn from_string(value: SectionInfo) -> Result<Boolean, SectionDecodeError> {
        match value.text.to_uppercase().as_str() {
            "false" => Ok(Boolean::False),
            "true" => Ok(Boolean::True),
            _ => Err(SectionDecodeError::Boolean {
                section: value.section,
            }),
        }
    }
}

#[derive(Clone)]
pub struct CellDefinitions {
    name: String,
    rgb_start: RgbColor,
    rgb_end: RgbColor,
}
impl CellDefinitions {
    pub fn new(line: String, line_num: usize) -> Result<Self, SectionDecodeError> {
        let mut sections = line.split(",").enumerate();
        let mut read = || {
            if let Some((section, text)) = sections.next() {
                Ok(SectionInfo {
                    text,
                    section,
                    line: line_num,
                })
            } else {
                Err(SectionDecodeError::NotEnoughSections { last_section: 0 })
            }
        };
        Ok(CellDefinitions {
            name: read()?.text.to_string(),
            rgb_start: RgbColor::from_hex(read()?)?,
            rgb_end: RgbColor::from_hex(read()?)?,
        })
    }
    pub fn color_ranges(&self) -> (&RgbColor, &RgbColor) {
        (&self.rgb_start, &self.rgb_end)
    }
}
pub struct CellDefinitionLoader {
    definitions: Vec<CellDefinitions>,
}
impl CellDefinitionLoader {
    pub fn new(path: PathBuf) -> Result<CellDefinitionLoader, LineDecodeError> {
        let file = File::open(&path).map_err(|_| LineDecodeError::CouldNotOpenFile(path))?;
        let reader = BufReader::new(file);
        Self::from_memory(reader)
    }
    pub fn from_memory<T: Read>(mem: BufReader<T>) -> Result<Self, LineDecodeError> {
        let mut definitions = vec![];
        let mut past_sections = None;
        for (index, line) in mem.lines().enumerate() {
            let line = line.map_err(|_| LineDecodeError::UnexpectedEOF)?;
            if let Some(pos) = line.find(';') {
                return Err(LineDecodeError::ContainsBadCharacter { line: index, pos });
            }
            if !line.starts_with('#') {
                let current_sections = line.split(",").count();
                if current_sections != past_sections.unwrap_or(current_sections) {
                    return Err(LineDecodeError::InconsistentSectionCount { line: index });
                }
                past_sections = Some(current_sections);
                match CellDefinitions::new(line, index) {
                    Ok(res) => definitions.push(res),
                    Err(error) => Err(LineDecodeError::BadSection { error, line: index })?,
                }
            }
        }
        Ok(Self { definitions })
    }

    pub fn name_array(&self) -> String {
        let mut collection = String::new();
        for definition in &self.definitions {
            if definition.name.contains(';') {
                panic!("Somehow definition contained a ';'")
            }
            collection.push_str(definition.name.as_str());
            collection.push(';');
        }
        collection
    }
    pub fn get(&self) -> &Vec<CellDefinitions> {
        &self.definitions
    }
}

pub enum SectionDecodeError {
    NotEnoughSections { last_section: usize },
    String { section: usize },
    Hex { section: usize },
    Boolean { section: usize },
    Float { section: usize },
    Integer { section: usize },
}
pub enum LineDecodeError {
    BadSection {
        error: SectionDecodeError,
        line: usize,
    },
    ContainsBadCharacter {
        line: usize,
        pos: usize,
    },
    InconsistentSectionCount {
        line: usize,
    },
    CouldNotOpenFile(PathBuf),
    UnexpectedEOF,
    NoPretextComment,
}

impl fmt::Display for SectionDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SectionDecodeError::NotEnoughSections { last_section } => {
                write!(
                    f,
                    "Not enough sections; last valid section index: {}",
                    last_section
                )
            }
            SectionDecodeError::String { section } => {
                write!(f, "Invalid string format in section {}", section)
            }
            SectionDecodeError::Hex { section } => {
                write!(f, "Invalid hexadecimal format in section {}", section)
            }
            SectionDecodeError::Boolean { section } => {
                write!(f, "Invalid boolean format in section {}", section)
            }
            SectionDecodeError::Float { section } => {
                write!(f, "Invalid float format in section {}", section)
            }
            SectionDecodeError::Integer { section } => {
                write!(f, "Invalid integer format in section {}", section)
            }
        }
    }
}

impl fmt::Display for LineDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LineDecodeError::BadSection { error, line } => {
                write!(f, "Error in line {}: {}", line, error)
            }
            LineDecodeError::ContainsBadCharacter { line, pos } => {
                write!(
                    f,
                    "Line {} contains a bad character at position {}",
                    line, pos
                )
            }
            LineDecodeError::InconsistentSectionCount { line } => {
                write!(f, "Inconsistent section count in line {}", line)
            }
            LineDecodeError::CouldNotOpenFile(path) => {
                write!(f, "Could not open file: {:?}", path)
            }
            LineDecodeError::UnexpectedEOF => {
                write!(f, "Unexpected end of file")
            }
            LineDecodeError::NoPretextComment => {
                write!(f, "No pretext comment found in the file")
            }
        }
    }
}
