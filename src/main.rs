extern crate byteorder;
extern crate clap;

use std::error::Error;
use std::fs::OpenOptions;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::io::SeekFrom;
use std::str;
use std::ops::Add;
use std::fs;
use std::io;
use std::fmt;

use byteorder::{ByteOrder, LittleEndian};
use clap::{App, Arg, ArgGroup};

const BUFFER_SIZE: usize = 1024 * 64;

#[derive(Debug)]
struct Header {
    mode: [u8; 5],
    record_length: i32,
    total_particles: i32,
    total_photons: i32,
    min_energy: f32,
    max_energy: f32,
    total_particles_in_source: f32
}

#[derive(Debug)]
struct Record {
    latch: u32,
    total_energy: f32,
    x_cm: f32,
    y_cm: f32,
    x_cos: f32, // TODO verify these are normalized
    y_cos: f32,
    weight: f32, // also carries the sign of the z direction, yikes
    zlast: Option<f32>
}

impl Header {
    fn length() -> usize { 25 }
    fn expected_bytes(&self) -> u64 { (self.total_particles as u64 + 1) * self.record_length as u64 }
    fn new_from_bytes(bytes: &[u8]) -> Result<Header, &'static str> {
        let mode = [0; 5];
        mode.clone_from_slice(&bytes[0..5]);
        let record_length = if &mode == b"MODE0" {
            28
        } else if &mode == b"MODE2" {
            32
        } else {
            return Err("First 5 bytes are invalid, must be MODE0 or MODE2")
        };
        Ok(Header {
            mode: mode,
            record_length: record_length,
            total_particles: LittleEndian::read_i32(&bytes[5..9]),
            total_photons: LittleEndian::read_i32(&bytes[9..13]),
            min_energy: LittleEndian::read_f32(&bytes[13..17]),
            max_energy: LittleEndian::read_f32(&bytes[17..21]),
            total_particles_in_source: LittleEndian::read_f32(&bytes[21..25]),
        })
    }
    fn write_to_bytes(&self, buffer: &mut [u8]) {
        buffer[0..5].clone_from_slice(&self.mode);
        LittleEndian::write_i32(&mut buffer[5..9], self.total_particles);
        LittleEndian::write_i32(&mut buffer[9..3], self.total_photons);
        LittleEndian::write_f32(&mut buffer[13..17], self.min_energy);
        LittleEndian::write_f32(&mut buffer[17..21], self.max_energy);
        LittleEndian::write_f32(&mut buffer[21..25], self.total_particles_in_source);
    }
    fn merge(&self, other: &Header) {
        assert!(&self.mode == &other.mode, "Merge mode mismatch");
        self.total_particles += other.total_particles;
        self.total_photons += other.total_photons;
        self.min_energy += other.min_energy;
        self.max_energy += other.max_energy;
        self.total_particles_in_source += other.total_particles_in_source;
    }
}


impl Record {
    fn new_from_bytes(buffer: &[u8], using_zlast: bool) -> Record {
        Record {
            latch: LittleEndian::read_u32(&buffer[0..4]),
            total_energy: LittleEndian::read_f32(&buffer[4..8]),
            x_cm: LittleEndian::read_f32(&buffer[8..12]),
            y_cm: LittleEndian::read_f32(&buffer[12..16]),
            x_cos: LittleEndian::read_f32(&buffer[16..20]),
            y_cos: LittleEndian::read_f32(&buffer[20..24]),
            weight: LittleEndian::read_f32(&buffer[24..28]),
            zlast: if using_zlast { Some(LittleEndian::read_f32(&buffer[28..32])) } else { None }
        }
    }
    fn write_to_bytes(&self, buffer: &mut [u8], using_zlast: bool) {
        LittleEndian::write_u32(&mut buffer[0..4], self.latch);
        LittleEndian::write_f32(&mut buffer[4..8], self.total_energy);
        LittleEndian::write_f32(&mut buffer[8..12], self.x_cm);
        LittleEndian::write_f32(&mut buffer[12..16], self.y_cm);
        LittleEndian::write_f32(&mut buffer[16..20], self.x_cos);
        LittleEndian::write_f32(&mut buffer[20..24], self.y_cos);
        LittleEndian::write_f32(&mut buffer[24..28], self.weight);
        if using_zlast { LittleEndian::write_f32(&mut buffer[28..32], self.weight); }
    }
    fn transform(buffer: &mut [u8], matrix: &[[f32; 2]]) {
        let mut x = LittleEndian::read_f32(&buffer[8..12]);
        let mut y = LittleEndian::read_f32(&buffer[12..16]);
        let mut x_cos = LittleEndian::read_f32(&buffer[16..20]);
        let mut y_cos = LittleEndian::read_f32(&buffer[20..24]);
        let x = x * matrix[0][0] + x * matrix[1][0];
        let y = y * matrix[0][1] + x * matrix[1][1];
        let x_cos = x_cos * matrix[0][0] + x_cos * matrix[1][0];
        let y_cos = y_cos * matrix[0][1] + y_cos * matrix[1][1];
        LittleEndian::write_f32(&mut buffer[8..12], x);
        LittleEndian::write_f32(&mut buffer[12..16], y);
        LittleEndian::write_f32(&mut buffer[16..20], x);
        LittleEndian::write_f32(&mut buffer[20..24], y);
    }

}

#[derive(Debug)]
enum EGSError {
    Io(io::Error),
    BadMode,
    BadLength
}

impl From<io::Error> for EGSError {
    fn from(err: io::Error) -> EGSError {
        EGSError::Io(err)
    }
}

impl fmt::Display for EGSError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EGSError::Io(ref err) => err.fmt(f),
            EGSError::BadMode => write!(f, "First 5 bytes of file are invalid, \
                                            must be MODE0 or MODE2"),
            EGSError::BadLength => write!(f, "Number of total particles does not\
                                             match byte length of file")
        }
    }
}

impl Error for EGSError {
    fn description(&self) -> &str {
        match *self {
            EGSError::Io(ref err) => err.description(),
            EGSError::BadMode => "invalid mode"
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            EGSError::Io(ref err) => Some(err),
            EGSError::BadMode => None,
            EGSError::BadLength => None
        }
    }
}

fn parse_header(path: &Path) -> Result<Header, EGSError> {
    let mut file = try!(File::open(&path));
    let mut buffer = [0; 25];
    try!(file.read_exact(&mut buffer));
    let header = try!(Header::new_from_bytes(&buffer));
    let metadata = try!(file.metadata().unwrap());
    if metadata.len() != header.expected_bytes() {
        Err(EGSError::BadLength)
    } else {
        Ok(header)
    }
}


fn combine(input_paths: &[&Path], output_path: &Path, delete_after_read: bool) -> Result<(), &'static String> {
    assert!(input_paths.len() > 0, "Cannot combine zero files");
    let path = input_paths[0];
    let mut header = try!(parse_header(&path));
    let mut final_header = header;
    for path in input_paths[1..].iter() {
        header = try!(parse_header(&path));
        if &header.mode != &final_header.mode {
            return Err(&format!("File {} has different mode/zlast than the initial file", path.display()))
        }
        final_header.merge(&header);
    }
    let mut out_file = try!(File::create(output_path)
        .map_err(|why| format!("Cannot create {} for writing: {}",
                               output_path.display(), why.description())));
    let mut buffer = [0; BUFFER_SIZE];
    final_header.write_to_bytes(&mut buffer);
    for path in input_paths.iter() {
        let mut in_file = try!(File::open(path)
            .map_err(|why| format!("Cannot open {} for full read: {}",
                                   path.display(), why.description())));
        in_file.seek(SeekFrom::Start(final_header.record_length as u64)).unwrap();
        let mut read = in_file.read(&mut buffer).unwrap();
        while read != 0 {
            out_file.write(&buffer).unwrap();
            read = in_file.read(&mut buffer).unwrap();
        }
        if delete_after_read {
            drop(in_file);
            try!(fs::remove_file(path).map_err(|why| format!(
                "Cannot remove input file {} after reading: {}", path.display(), why.description())));
        }
    };
}

fn transform(input_path: &Path, output_path: &Path, matrix: &[[f32; 2]]) -> Result<(), &'static str> {
    // transform should take a read/write file handle? no
    // here we can just copy the file, then transform in place?
    // or, we can read it in, and write it somewhere else...
    let header = try!(parse_header(input_path));
    let mut input_file = match File::open(&input_path) {
        Err(why) => return Err(&format!("Couldn't open {}: {}", input_path.display(), why.description())),
        Ok(file) => file
    };
    let mut output_file = match File::open(&output_path) {
        Err(why) => return Err(&format!("Couldn't open {} for writing: {}", output_path.display(), why.description())),
        Ok(file) => file
    };
    let mut buffer = [0; BUFFER_SIZE];
    let mut read = input_file.read(&mut buffer).unwrap();
    let mut offset = header.record_length as usize;
    let mut position = 0;
    while read != 0 {
        let number_records = (read - offset) / header.record_length as usize;
        for i in 0..number_records {
            let index = offset + i * header.record_length as usize;
            Record::transform(&mut buffer[index..], &matrix);
        }
        offset = (read - offset) % header.record_length as usize;
        output_file.write(&buffer[..read]).unwrap();
        position += read as u64;
        read = input_file.read(&mut buffer).unwrap();
    };
    // here we should verify we read the right number of records, like this:
    assert!(offset == 0, "Offset should be zero at end of reading");
    let records_read = position / header.record_length as u64 - header.record_length as u64;
    assert!(records_read == header.total_particles as u64, "Records read should equal ");
}

fn transform_in_place(path: &Path, matrix: &[[f32; 2]]) -> Result<(), &'static str> {
    let header = try!(parse_header(path));
    let mut file = match OpenOptions::new().read(true).write(true).open(&path) {
        Err(why) => return Err(&format!("Couldn't open {}: {}", path.display(), why.description())),
        Ok(file) => file
    };
    let mut buffer = [0; BUFFER_SIZE];
    let mut read = file.read(&mut buffer).unwrap();
    let mut offset = header.record_length as usize;
    let mut position = 0;
    while read != 0 {
        let number_records = (read - offset) / header.record_length as usize;
        for i in 0..number_records {
            let index = offset + i * header.record_length as usize;
            Record::transform(&mut buffer[index..], &matrix);
        }
        offset = (read - offset) % header.record_length as usize;
        position = file.seek(SeekFrom::Start(position)).unwrap();
        file.write(&buffer[..read]).unwrap();
        position += read as u64;
        read = file.read(&mut buffer).unwrap();
    };
    // here we should verify we read the right number of records, like this:
    assert!(offset == 0, "Offset should be zero at end of reading");
    let records_read = position / header.record_length as u64 - header.record_length as u64;
    assert!(records_read == header.total_particles as u64, &"Records read should equal");
}

fn main() {
    let matches = App::new("rbeamdp")
        .version("0.1")
        .author("Henry B. <henry.baxter@gmail.com>")
        .about("Supplement to beamdp for combining and transforming egsphsp (EGS phase space) files")
        .arg(Arg::with_name("translate")
            .help("Translate using -x and -y (in centimeters)"))
        .arg(Arg::with_name("rotate")
            .help("Rotate by -theta radians counter clockwise around z axis"))
        .arg(Arg::with_name("reflect")
            .help("Reflect in vector specified with -x and -y"))
        .arg(Arg::with_name("in-place")
            .help("Transform input file in-place (not for use with --combine)"))
        .arg(Arg::with_name("combine")
            .help("Combine input files and write to output file - does not adjust weights"))
        .group(ArgGroup::with_name("command")
            .args(&["translate", "rotate", "reflect", "combine"])
            .required(true));
    //let path = Path::new("input.egsphsp2");
    //combine(&[path], Path::new("output.egsphsp"), false);
}
