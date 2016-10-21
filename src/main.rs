extern crate byteorder;
#[macro_use]
extern crate clap;

use std::error::Error;
use std::fs::OpenOptions;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::io::SeekFrom;
use std::str;
use std::fs;
use std::io;
use std::fmt;
use std::f64::consts;

use byteorder::{ByteOrder, LittleEndian};
use clap::{App, AppSettings, SubCommand, Arg};

const BUFFER_SIZE: usize = 1024 * 64;
const HEADER_LENGTH: usize = 25;

#[derive(Debug)]
struct Header {
    mode: [u8; 5],
    record_length: i32,
    total_particles: i32,
    total_photons: i32,
    min_energy: f32,
    max_energy: f32,
    total_particles_in_source: f32,
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
    zlast: Option<f32>,
}

#[derive(Debug)]
enum EGSError {
    Io(io::Error),
    BadMode,
    BadLength,
    ModeMismatch,
}

type EGSResult<T> = Result<T, EGSError>;

impl From<io::Error> for EGSError {
    fn from(err: io::Error) -> EGSError {
        EGSError::Io(err)
    }
}

impl fmt::Display for EGSError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EGSError::Io(ref err) => err.fmt(f),
            EGSError::BadMode => {
                write!(f,
                       "First 5 bytes of file are invalid, must be MODE0 or MODE2")
            }
            EGSError::BadLength => {
                write!(f,
                       "Number of total particles does notmatch byte length of file")
            }
            EGSError::ModeMismatch => write!(f, "Input file MODE0/MODE2 do not match"),
        }
    }
}

impl Error for EGSError {
    fn description(&self) -> &str {
        match *self {
            EGSError::Io(ref err) => err.description(),
            EGSError::BadMode => "invalid mode",
            EGSError::BadLength => "bad file length",
            EGSError::ModeMismatch => "mode mismatch",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            EGSError::Io(ref err) => Some(err),
            EGSError::BadMode => None,
            EGSError::BadLength => None,
            EGSError::ModeMismatch => None,
        }
    }
}

impl Header {
    fn expected_bytes(&self) -> u64 {
        (self.total_particles as u64 + 1) * self.record_length as u64
    }
    fn new_from_bytes(bytes: &[u8]) -> EGSResult<Header> {
        let mut mode = [0; 5];
        mode.clone_from_slice(&bytes[..5]);
        let record_length = if &mode == b"MODE0" {
            28
        } else if &mode == b"MODE2" {
            32
        } else {
            return Err(EGSError::BadMode);
        };
        Ok(Header {
            mode: mode,
            record_length: record_length,
            total_particles: LittleEndian::read_i32(&bytes[5..9]),
            total_photons: LittleEndian::read_i32(&bytes[9..13]),
            max_energy: LittleEndian::read_f32(&bytes[13..17]),
            min_energy: LittleEndian::read_f32(&bytes[17..21]),
            total_particles_in_source: LittleEndian::read_f32(&bytes[21..25]),
        })
    }
    fn write_to_bytes(&self, buffer: &mut [u8]) {
        buffer[0..5].clone_from_slice(&self.mode);
        LittleEndian::write_i32(&mut buffer[5..9], self.total_particles);
        LittleEndian::write_i32(&mut buffer[9..13], self.total_photons);
        LittleEndian::write_f32(&mut buffer[13..17], self.max_energy);
        LittleEndian::write_f32(&mut buffer[17..21], self.min_energy);
        LittleEndian::write_f32(&mut buffer[21..25], self.total_particles_in_source);
    }
    fn merge(&mut self, other: &Header) {
        assert!(&self.mode == &other.mode, "Merge mode mismatch");
        self.total_particles += other.total_particles;
        self.total_photons += other.total_photons;
        self.min_energy = self.min_energy.min(other.min_energy);
        self.max_energy = self.max_energy.max(other.max_energy);
        self.total_particles_in_source += other.total_particles_in_source;
    }
}


impl Record {
    // fn new_from_bytes(buffer: &[u8], using_zlast: bool) -> Record {
    // Record {
    // latch: LittleEndian::read_u32(&buffer[0..4]),
    // total_energy: LittleEndian::read_f32(&buffer[4..8]),
    // x_cm: LittleEndian::read_f32(&buffer[8..12]),
    // y_cm: LittleEndian::read_f32(&buffer[12..16]),
    // x_cos: LittleEndian::read_f32(&buffer[16..20]),
    // y_cos: LittleEndian::read_f32(&buffer[20..24]),
    // weight: LittleEndian::read_f32(&buffer[24..28]),
    // zlast: if using_zlast { Some(LittleEndian::read_f32(&buffer[28..32])) } else { None }
    // }
    // }
    // fn write_to_bytes(&self, buffer: &mut [u8], using_zlast: bool) {
    // LittleEndian::write_u32(&mut buffer[0..4], self.latch);
    // LittleEndian::write_f32(&mut buffer[4..8], self.total_energy);
    // LittleEndian::write_f32(&mut buffer[8..12], self.x_cm);
    // LittleEndian::write_f32(&mut buffer[12..16], self.y_cm);
    // LittleEndian::write_f32(&mut buffer[16..20], self.x_cos);
    // LittleEndian::write_f32(&mut buffer[20..24], self.y_cos);
    // LittleEndian::write_f32(&mut buffer[24..28], self.weight);
    // if using_zlast { LittleEndian::write_f32(&mut buffer[28..32], self.weight); }
    // }
    //
    fn transform(buffer: &mut [u8], matrix: &[[f32; 3]; 3]) {
        let mut x = LittleEndian::read_f32(&buffer[8..12]);
        let mut y = LittleEndian::read_f32(&buffer[12..16]);
        let mut x_cos = LittleEndian::read_f32(&buffer[16..20]);
        let mut y_cos = LittleEndian::read_f32(&buffer[20..24]);
        x = matrix[0][0] * x + matrix[0][1] * y + matrix[0][2] * 1.0;
        y = matrix[1][0] * x + matrix[1][1] * y + matrix[2][0] * 1.0;
        x_cos = matrix[0][0] * x_cos + matrix[0][1] * y_cos + matrix[0][2] * 1.0;
        y_cos = matrix[1][0] * x_cos + matrix[1][1] * y_cos + matrix[1][2] * 1.0;
        LittleEndian::write_f32(&mut buffer[8..12], x);
        LittleEndian::write_f32(&mut buffer[12..16], y);
        LittleEndian::write_f32(&mut buffer[16..20], x_cos);
        LittleEndian::write_f32(&mut buffer[20..24], y_cos);
    }
}


fn parse_header(path: &Path) -> EGSResult<Header> {
    let mut file = try!(File::open(&path));
    let mut buffer = [0; HEADER_LENGTH];
    try!(file.read_exact(&mut buffer));
    let header = try!(Header::new_from_bytes(&buffer));
    let metadata = try!(file.metadata());
    if metadata.len() != header.expected_bytes() {
        Err(EGSError::BadLength)
    } else {
        Ok(header)
    }
}


fn combine(input_paths: &[&Path], output_path: &Path, delete_after_read: bool) -> EGSResult<()> {
    assert!(input_paths.len() > 0, "Cannot combine zero files");
    let path = input_paths[0];
    let mut header = try!(parse_header(&path));
    let mut final_header = header;
    for path in input_paths[1..].iter() {
        header = try!(parse_header(&path));
        if &header.mode != &final_header.mode {
            return Err(EGSError::ModeMismatch);
        }
        final_header.merge(&header);
    }
    println!("final_header = {:?}", final_header);
    let mut out_file = try!(File::create(output_path));
    let mut buffer = [0; BUFFER_SIZE];
    final_header.write_to_bytes(&mut buffer);
    let offset = final_header.record_length as usize;
    try!(out_file.write(&buffer[..offset]));
    for path in input_paths.iter() {
        let mut in_file = try!(File::open(path));
        try!(in_file.seek(SeekFrom::Start(offset as u64)));
        let mut read = try!(in_file.read(&mut buffer));
        while read != 0 {
            try!(out_file.write(&buffer[..read]));
            read = try!(in_file.read(&mut buffer));
        }
        if delete_after_read {
            drop(in_file);
            try!(fs::remove_file(path));
        }
    }
    Ok(())
}

fn transform(input_path: &Path, output_path: &Path, matrix: &[[f32; 3]; 3]) -> EGSResult<()> {
    let header = try!(parse_header(input_path));
    let mut input_file = try!(File::open(&input_path));
    let mut output_file = try!(File::create(&output_path));
    let mut buffer = [0; BUFFER_SIZE];
    let mut read = try!(input_file.read(&mut buffer));
    let mut offset = header.record_length as usize;
    while read != 0 {
        let number_records = (read - offset) / header.record_length as usize;
        for i in 0..number_records {
            let index = offset + i * header.record_length as usize;
            Record::transform(&mut buffer[index..], &matrix);
        }
        offset = (read - offset) % header.record_length as usize;
        try!(output_file.write(&buffer[..read]));
        read = try!(input_file.read(&mut buffer));
    }
    Ok(())
}

fn transform_in_place(path: &Path, matrix: &[[f32; 3]; 3]) -> EGSResult<()> {
    let header = try!(parse_header(path));
    let mut file = try!(OpenOptions::new().read(true).write(true).open(&path));
    let mut buffer = [0; BUFFER_SIZE];
    let mut read = try!(file.read(&mut buffer));
    let mut offset = header.record_length as usize;
    let mut position = 0;
    while read != 0 {
        let number_records = (read - offset) / header.record_length as usize;
        for i in 0..number_records {
            let index = offset + i * header.record_length as usize;
            Record::transform(&mut buffer[index..], &matrix);
        }
        offset = (read - offset) % header.record_length as usize;
        position = try!(file.seek(SeekFrom::Start(position)));
        try!(file.write(&buffer[..read]));
        position += read as u64;
        read = try!(file.read(&mut buffer));
    }
    Ok(())
}

struct Transform;

impl Transform {
    fn reflection(matrix: &mut [[f32; 3]; 3], x_raw: f32, y_raw: f32) {
        let norm = (x_raw * x_raw + y_raw * y_raw).sqrt();
        let x = x_raw / norm;
        let y = y_raw / norm;
        *matrix =
            [[x * x - y * y, 2.0 * x * y, 0.0], [2.0 * x * y, y * y - x * x, 0.0], [0.0, 0.0, 1.0]];
    }
    fn translation(matrix: &mut [[f32; 3]; 3], x: f32, y: f32) {
        *matrix = [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]];
    }
    fn rotation(matrix: &mut [[f32; 3]; 3], theta: f32) {
        *matrix =
            [[theta.cos(), -theta.sin(), 0.0], [theta.cos(), theta.sin(), 0.0], [0.0, 0.0, 1.0]];
    }
}

#[allow(dead_code)]
fn identical(path1: &Path, path2: &Path) -> bool {
    let mut file1 = File::open(path1).unwrap();
    let mut file2 = File::open(path2).unwrap();
    let mut buf1 = Vec::new();
    let mut buf2 = Vec::new();
    file1.read_to_end(&mut buf1).unwrap();
    file2.read_to_end(&mut buf2).unwrap();
    buf1.as_slice() == buf2.as_slice()
}

#[test]
fn first_file_header_correct() {
    let path = Path::new("test_data/first.egsphsp");
    let header = parse_header(path).unwrap();
    assert!(header.record_length == 28);
    assert!(header.total_particles == 352, format!("Total particles incorrect, found {:?}", header.total_particles));
    assert!(header.total_photons == 303, format!("Total photons incorrect, found {:?}", header.total_photons));
    assert!(header.max_energy - 0.1988 < 0.0001, format!("Max energy incorrect, found {:?}", header.max_energy));
    assert!(header.min_energy - 0.0157 < 0.0001, format!("Min energy incorrect, found {:?}", header.min_energy));
    assert!(header.total_particles_in_source - 100.0 < 0.0001, format!("Total particles in source incorrect, found {:?}", header.total_particles_in_source));
    // open the first one and make sure the entries are valid
}

#[test]
fn second_file_header_correct() {
    let path = Path::new("test_data/second.egsphsp");
    let header = parse_header(path).unwrap();
    assert!(header.record_length == 28);
    assert!(header.total_particles == 352, format!("Total particles incorrect, found {:?}, header.total_particles", header.total_particles));
    assert!(header.total_photons == 303, format!("Total photons incorrect, found {:?}", header.total_photons));
    assert!(header.max_energy - 0.1988 < 0.0001, format!("Max energy incorrect, found {:?}", header.max_energy));
    assert!(header.min_energy - 0.0157 < 0.0001, format!("Min energy incorrect, found {:?}", header.min_energy));
    assert!(header.total_particles_in_source - 100.0 < 0.0001, format!("Total particles in source incorrect, found {:?}", header.total_particles_in_source));
    // open the first one and make sure the entries are valid
}

#[test]
fn combined_file_header_correct() {
    let path = Path::new("test_data/combined.egsphsp");
    let header = parse_header(path).unwrap();
    assert!(header.record_length == 28);
    assert!(header.total_particles == 352 * 2, format!("Total particles incorrect, found {:?}, header.total_particles", header.total_particles));
    assert!(header.total_photons == 303 * 2, format!("Total photons incorrect, found {:?}", header.total_photons));
    assert!(header.max_energy - 0.1988 < 0.0001, format!("Max energy incorrect, found {:?}", header.max_energy));
    assert!(header.min_energy - 0.0157 < 0.0001, format!("Min energy incorrect, found {:?}", header.min_energy));
    assert!(header.total_particles_in_source - 100.0 * 2.0 < 0.0001, format!("Total particles in source incorrect, found {:?}", header.total_particles_in_source));
    // open the first one and make sure the entries are valid
}

#[test]
fn combine_operation_matches_beamdp() {
    let input_paths = vec![Path::new("test_data/first.egsphsp"), Path::new("test_data/second.egsphsp")];
    let output_path = Path::new("test_data/output_combined.egsphsp");
    let expected_path = Path::new("test_data/combined.egsphsp");
    combine(&input_paths, output_path, false).unwrap();
    assert!(identical(output_path, expected_path));
}

#[test]
fn translate_operation() {
    let input_path = Path::new("test_data/first.egsphsp");
    let output_path = Path::new("test_data/translated.egsphsp");
    let x = 5.0;
    let y = 5.0;
    let mut matrix = [[0.0; 3]; 3];
    Transform::translation(&mut matrix, x, y);
    transform(input_path, output_path, &matrix).unwrap();
    Transform::translation(&mut matrix, -x, -y);
    transform_in_place(output_path, &matrix).unwrap();
    assert!(identical(input_path, output_path));
}

#[test]
fn rotate_operation() {
    let input_path = Path::new("test_data/first.egsphsp");
    let output_path = Path::new("test_data/rotated.egsphsp");
    let mut matrix = [[0.0; 3]; 3];
    Transform::rotation(&mut matrix, consts::PI as f32);
    transform(input_path, output_path, &matrix).unwrap();
    Transform::rotation(&mut matrix, consts::PI as f32);
    transform_in_place(output_path, &matrix).unwrap();
    assert!(identical(input_path, output_path));
}

#[test]
fn reflect_operation() {
    let input_path = Path::new("test_data/first.egsphsp");
    let output_path = Path::new("test_data/reflected.egsphsp");
    let mut matrix = [[0.0; 3]; 3];
    Transform::reflection(&mut matrix, 1.0, 0.0);
    transform(input_path, output_path, &matrix).unwrap();
    Transform::translation(&mut matrix, 1.0, 0.0);
    transform_in_place(output_path, &matrix).unwrap();
    assert!(identical(input_path, output_path));
}


/*
    well this was supposed to be a fast one that uses constant memory but who has the time
fn identical(path1: &Path, path2: &Path) -> bool {
    let mut file1 = File::open(path1).unwrap();
    let mut file2 = File::open(path2).unwrap();
    let mut buffer1 = [0; BUFFER_SIZE];
    let mut buffer2 = [0; BUFFER_SIZE];
    let mut offset_buffer = [0; BUFFER_SIZE];
    let mut read1 = file1.read(&mut buffer1).unwrap();
    let mut read2 = file2.read(&mut buffer2).unwrap();
    let mut offset1;
    let mut offset2;
    while read1 != 0 && read2 != 0 {
        let read_both = cmp::min(read1, read2);
        offset1 = read1 - read_both;
        offset2 = read2 - read_both;
        if buffer1[..read_both] != buffer2[..read_both] {
            return false;
        };
        offset_buffer.clone_from_slice(&buffer1[read_both..read_both + offset1]);
        buffer1.clone_from_slice(&offset_buffer[..offset1]);
        offset_buffer.clone_from_slice(&buffer2[read_both..read_both + offset2]);
        buffer2.clone_from_slice(&offset_buffer[..offset2]);
        read1 = file1.read(&mut buffer1[offset1..]).unwrap();
        read2 = file2.read(&mut buffer2[offset2..]).unwrap();
    };
    buffer1[..read_both] == buffer2[..read_both]
}
*/



fn main() {
    let matches = App::new("beamdpr")
        .version("0.1")
        .author("Henry B. <henry.baxter@gmail.com>")
        .about("Supplement to beamdp for combining and transforming egsphsp (EGS phase space) \
                files")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("combine")
            .about("Combine phase space from one or more input files into outputfile - does not \
                    adjust weights")
            .arg(Arg::with_name("input")
                .required(true)
                .multiple(true))
            .arg(Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true)
                .required(true)))
        .subcommand(SubCommand::with_name("translate")
            .about("Translate using X and Y (in centimeters)")
            .arg(Arg::with_name("in-place")
                .short("i")
                .long("in-place")
                .help("Transform input file in-place"))
            .arg(Arg::with_name("x")
                .short("x")
                .takes_value(true)
                .required_unless("y"))
            .arg(Arg::with_name("y")
                .short("y")
                .takes_value(true)
                .required_unless("x"))
            .arg(Arg::with_name("input")
                .help("Phase space file")
                .required(true))
            .arg(Arg::with_name("output")
                .help("Output file")
                .required_unless("in-place")))
        .subcommand(SubCommand::with_name("rotate")
            .about("Rotate by --angle radians counter clockwise around z axis")
            .arg(Arg::with_name("in-place")
                .short("i")
                .long("in-place")
                .help("Transform input file in-place"))
            .arg(Arg::with_name("angle")
                .short("a")
                .long("angle")
                .takes_value(true)
                .required(true)
                .help("Counter clockwise angle in radians to rotate around Z axis"))
            .arg(Arg::with_name("input")
                .help("Phase space file")
                .required(true))
            .arg(Arg::with_name("output")
                .help("Output file")
                .required_unless("in-place")))
        .subcommand(SubCommand::with_name("reflect")
            .about("Reflect in vector specified with -x and -y")
            .arg(Arg::with_name("in-place")
                .short("i")
                .long("in-place")
                .help("Transform input file in-place"))
            .arg(Arg::with_name("x")
                .short("x")
                .takes_value(true)
                .required_unless("x")
                .default_value("0"))
            .arg(Arg::with_name("y")
                .short("y")
                .takes_value(true)
                .required_unless("y")
                .default_value("0"))
            .arg(Arg::with_name("input")
                .help("Phase space file")
                .required(true))
            .arg(Arg::with_name("output")
                .help("Output file")
                .required_unless("in-place")))
        .get_matches();
    let result = if matches.subcommand_name().unwrap() == "combine" {
        let sub_matches = matches.subcommand_matches("combine").unwrap();
        let input_paths: Vec<&Path> = sub_matches.values_of("input")
            .unwrap()
            .map(|s| Path::new(s))
            .collect();
        let output_path = Path::new(sub_matches.value_of("output").unwrap());
        combine(&input_paths,
                output_path,
                sub_matches.is_present("delete-after"))
    } else {
        let mut matrix = [[0.0; 3]; 3];
        match matches.subcommand_name().unwrap() {
            "translate" => {
                let sub_matches = matches.subcommand_matches("translate").unwrap();
                let x = value_t!(sub_matches, "x", f32).unwrap();
                let y = value_t!(sub_matches, "y", f32).unwrap();
                Transform::translation(&mut matrix, x, y);
                let input_path = Path::new(sub_matches.value_of("input").unwrap());
                if sub_matches.is_present("in-place") {
                    transform_in_place(input_path, &matrix)
                } else {
                    let output_path = Path::new(sub_matches.value_of("input").unwrap());
                    transform(input_path, output_path, &matrix)
                }
            }
            "reflect" => {
                let sub_matches = matches.subcommand_matches("reflect").unwrap();
                let x = value_t!(sub_matches, "x", f32).unwrap();
                let y = value_t!(sub_matches, "y", f32).unwrap();
                Transform::reflection(&mut matrix, x, y);
                let input_path = Path::new(sub_matches.value_of("input").unwrap());
                if sub_matches.is_present("in-place") {
                    transform_in_place(input_path, &matrix)
                } else {
                    let output_path = Path::new(sub_matches.value_of("input").unwrap());
                    transform(input_path, output_path, &matrix)
                }
            }
            "rotate" => {
                let sub_matches = matches.subcommand_matches("rotate").unwrap();
                let angle = value_t!(sub_matches, "angle", f32).unwrap();
                Transform::rotation(&mut matrix, angle);
                let input_path = Path::new(sub_matches.value_of("input").unwrap());
                if sub_matches.is_present("in-place") {
                    transform_in_place(input_path, &matrix)
                } else {
                    let output_path = Path::new(sub_matches.value_of("input").unwrap());
                    transform(input_path, output_path, &matrix)
                }
            }
            _ => panic!("Programmer error, trying to match invalid command"),
        }
    };

    match result {
        Ok(()) => println!("Done :)"),
        Err(err) => println!("Problem: {}", err.description()),
    };
}
