use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use libc::pid_t;
use pest::Parser as ParserTrait;
use pest_derive::Parser;
use phf::phf_map;
use serde::{Deserialize, Serialize};

pub mod error;
use error::Error;

static PSUEDO_PATH_MAP: phf::Map<&'static str, Pathname> = phf_map! {
    "[stack]" => Pathname::Stack,
    "[vdso]" => Pathname::Vdso,
    "[vvar]" => Pathname::Vvar,
    "[vsyscall]" => Pathname::Vsyscall,
    "[heap]" => Pathname::Heap,
};

#[derive(Parser)]
#[grammar = "map.pest"]
struct MapParser;

/// Represents the variants of the "pathname" field in a map.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum Pathname {
    Stack,
    Vdso,
    Vvar,
    Vsyscall,
    Heap,
    Mmap,
    OtherPseudo(String),
    // NOTE(ww): This should really be a PathBuf, but pest uses UTF-8 strings.
    // Better hope your paths are valid UTF-8!
    Path(String),
}

/// Represents the address range of a map.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AddressRange {
    pub begin: u64,
    pub end: u64,
}

impl fmt::Display for AddressRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x}-{:x}", self.begin, self.end)
    }
}

/// Represents the permissions associated with a map.
#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Permissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub shared: bool,
    pub private: bool,
}

impl fmt::Display for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut mask = String::new();

        if self.readable {
            mask.push('r');
        } else {
            mask.push('-');
        }

        if self.writable {
            mask.push('w');
        } else {
            mask.push('-');
        }

        if self.executable {
            mask.push('e');
        } else {
            mask.push('-');
        }

        if self.shared {
            mask.push('s')
        } else {
            mask.push('p')
        }

        write!(f, "{}", mask)
    }
}

/// Represents the device associated with a map.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Device {
    pub major: u64,
    pub minor: u64,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02}-{:02}", self.major, self.minor)
    }
}

/// Represents a map, i.e. a region of program memory.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Map {
    /// The map's address range.
    pub address_range: AddressRange,

    /// The map's permissions.
    pub permissions: Permissions,

    /// The offset of the map within its source.
    pub offset: u64,

    /// The device that the map's inode belongs on.
    pub device: Device,

    /// The map's inode (or 0 if inapplicable).
    pub inode: u64,

    /// The map's pathname field.
    pub pathname: Pathname,
}

impl Default for Map {
    fn default() -> Self {
        Map {
            address_range: AddressRange { begin: 0, end: 0 },
            permissions: Default::default(),
            offset: 0,
            device: Device { major: 0, minor: 0 },
            inode: 0,
            pathname: Pathname::Mmap,
        }
    }
}

impl Map {
    fn parse(line: &str) -> Result<Map, Error> {
        // NOTE(ww): The map rule is singular, so this next + unwrap is safe after
        // a successful parse.
        let parsed = MapParser::parse(Rule::map, line)?.next().unwrap();
        let mut map: Map = Default::default();

        for entry in parsed.into_inner() {
            match entry.as_rule() {
                Rule::address_range => {
                    let mut address_range = entry.into_inner();
                    map.address_range.begin =
                        u64::from_str_radix(address_range.next().unwrap().as_str(), 16)?;
                    map.address_range.end =
                        u64::from_str_radix(address_range.next().unwrap().as_str(), 16)?;
                }
                Rule::permissions => {
                    let permissions = entry.as_str().as_bytes();

                    map.permissions.readable = permissions[0] == b'r';
                    map.permissions.writable = permissions[1] == b'w';
                    map.permissions.executable = permissions[2] == b'x';
                    map.permissions.shared = permissions[3] == b's';
                    map.permissions.private = !map.permissions.shared;
                }
                Rule::offset => {
                    let offset = entry.as_str();
                    map.offset = u64::from_str_radix(offset, 16)?;
                }
                Rule::device => {
                    let mut device = entry.into_inner();

                    map.device.major = u64::from_str_radix(device.next().unwrap().as_str(), 16)?;
                    map.device.minor = u64::from_str_radix(device.next().unwrap().as_str(), 16)?;
                }
                Rule::inode => {
                    map.inode = entry.as_str().parse()?;
                }
                Rule::pathname => {
                    let pathname = entry.as_str();

                    if pathname.is_empty() {
                        // An empty path indicates an mmap'd region.
                        map.pathname = Pathname::Mmap;
                    } else if PSUEDO_PATH_MAP.contains_key(pathname) {
                        // There are some pseudo-files that we know; use their enum variants
                        // if we see them.
                        map.pathname = PSUEDO_PATH_MAP.get(pathname).unwrap().clone();
                    } else if pathname.starts_with('[') && pathname.ends_with(']') {
                        // There are probably other pseudo-files that we don't know;
                        // if we see something that looks like one, mark it as such.
                        map.pathname = Pathname::OtherPseudo(pathname.into());
                    } else {
                        // Finally, treat anything else like a path.
                        // As proc(5) notes, there are a few ambiguities here with escaped
                        // newlines and the "(deleted)" suffix; leave these to the user to figure out.
                        map.pathname = Pathname::Path(pathname.into());
                    }
                }
                // NOTE(ww): There are other rules, but we should never be able to match them in this context.
                _ => {
                    unreachable!();
                }
            }
        }

        Ok(map)
    }
}

/// A wrapper structure for consuming individual `Map`s from a reader.
pub struct Maps<T: BufRead> {
    reader: T,
}

impl<T: BufRead> Iterator for Maps<T> {
    type Item = Result<Map, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line_buf = String::new();
        match self.reader.read_line(&mut line_buf) {
            Ok(0) => None,
            Ok(_) => {
                // NOTE(ww): Annoying: the Lines iterator yields lines
                // without their trailing delimiters, but read_line includes them.
                if line_buf.ends_with('\n') {
                    line_buf.pop();
                }
                Some(Map::parse(&line_buf))
            }
            Err(e) => Some(Err(e.into())),
        }
    }
}

impl<T: BufRead> Maps<T> {
    /// Creates a new `Maps` from the given `reader`.
    pub fn new(reader: T) -> Maps<T> {
        Maps { reader }
    }
}

/// Returns an iterable `Maps` for the given pid.
pub fn from_pid(pid: pid_t) -> Result<Maps<BufReader<File>>, Error> {
    let path = Path::new("/proc").join(pid.to_string()).join("maps");
    from_path(&path)
}

/// Returns an iterable `Maps` parsed from the given file.
pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Maps<BufReader<File>>, Error> {
    let reader = {
        let f = File::open(path)?;
        BufReader::new(f)
    };

    Ok(Maps::new(reader))
}

/// Returns an iterable `Maps` parsed from the given string.
pub fn from_str<'a>(maps_data: &'a str) -> Maps<&'a [u8]> {
    Maps::new(maps_data.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use glob::glob;
    use serde_json;

    #[test]
    fn test_parse_map() {
        let map =
            Map::parse("5608dd391000-5608dd3be000 r--p 00000000 08:11 6572575 /bin/bash").unwrap();

        assert_eq!(map.address_range.begin, 0x5608dd391000);
        assert_eq!(map.address_range.end, 0x5608dd3be000);

        assert!(map.permissions.readable);
        assert!(!map.permissions.writable);
        assert!(!map.permissions.executable);
        assert!(!map.permissions.shared);
        assert!(map.permissions.private);

        assert_eq!(map.offset, 0);

        assert_eq!(map.device.major, 8);
        assert_eq!(map.device.minor, 17);

        assert_eq!(map.inode, 6572575);

        assert_eq!(map.pathname, Pathname::Path("/bin/bash".into()));
    }

    #[test]
    fn test_reference_inputs() {
        let test_data = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");

        for maps_input in glob(test_data.join("*.maps").to_str().unwrap()).unwrap() {
            let maps_input = maps_input.unwrap();
            let reference_output = maps_input.with_extension("json");

            let maps = from_path(&maps_input).unwrap().collect::<Vec<_>>();
            let expected_maps: Vec<Map> =
                serde_json::from_str(&fs::read_to_string(reference_output).unwrap()).unwrap();

            assert_eq!(maps.len(), expected_maps.len());
            for (map, emap) in maps.iter().zip(expected_maps.iter()) {
                assert_eq!(map.as_ref().unwrap(), emap);
            }
        }

        // TODO(ww): Add some invalid reference inputs.
    }
}
