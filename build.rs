use std::env;
use std::fs;
use std::path::Path;

fn parse_hex(hex: &str) -> Result<[u8; 3], &'static str> {
    if hex.len() != 7 {
        return Err("Hex color code must be 6 characters long");
    }
    if &hex[0..1] != "#" {
        return Err("needs # symbol at sttart of hex");
    }
    let r = u8::from_str_radix(&hex[1..3], 16).map_err(|_| "Invalid red component")?;
    let g = u8::from_str_radix(&hex[3..5], 16).map_err(|_| "Invalid green component")?;
    let b = u8::from_str_radix(&hex[5..7], 16).map_err(|_| "Invalid blue component")?;

    Ok([r, g, b])
}
fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("cell_settings.rs");

    let enum_definitions =
        fs::read_to_string("assets/cell-definitions.csv").expect("Failed to read settings file");
    let mut parsed_lines = vec![];
    for line in enum_definitions.lines() {
        let parts: Vec<_> = line.split(' ').collect();

        let (i, rest) = parts[0].split_at(1);
        let name = format!("{}{}", i.to_uppercase(), rest);

        let character = parts[1];
        let rgb_start = parse_hex(parts[2]).unwrap();
        let rgb_end = parse_hex(parts[3]).unwrap();

        parsed_lines.push((name, character, rgb_start, rgb_end))
    }

    let length = parsed_lines.len();

    let names = parsed_lines
        .iter()
        .map(|(name, _, _, _)| format!("{name},"))
        .collect::<Vec<String>>()
        .join("\n");

    let colors = parsed_lines
        .iter()
        .map(|(_, _, start, end)| format!("[{start:?}, {end:?}]"))
        .collect::<Vec<String>>()
        .join(",");

    let inputs = parsed_lines
        .iter()
        .map(|(name, character, _, _)| {
            format!("VirtualKeyCode::{character} => *self = CellType::{name}")
        })
        .collect::<Vec<String>>()
        .join(",");

    let definition = format!(
        "
        use winit::event::VirtualKeyCode;
        const COLOR_LOOKUP: [[[u8;3];2];{length}] = [{colors}];
        #[repr(usize)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum CellType {{
            {names}
        }}
        impl CellType {{
            pub fn color(&self) -> &[[u8;3];2]  {{
                unsafe {{
                    &COLOR_LOOKUP[(*self as usize)]
                }}
            }}
            pub fn switch_if_valid(&mut self, code: VirtualKeyCode) {{
                match code {{
                    {inputs},
                    _ => ()
                }}         
            }}
        }}
        "
    );

    fs::write(&dest_path, definition).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/cell-definitions.csv");
}
