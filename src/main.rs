extern crate byteorder;
extern crate clap;

use std::error::Error;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;
use std::io::SeekFrom;

use byteorder::{ByteOrder, LittleEndian};

fn main() {

    // get the input path
    let path = Path::new("big.egsphsp2");
    let display = path.display();

    // get the translation amounts
    let x_translate = 2.0;
    let y_translate = 1.0;

    // open the file for read/write
	let mut file = match OpenOptions::new().read(true).write(true).open(&path) {
        Err(why) => panic!("Couldn't open {}: {}", display, why.description()),
        Ok(file) => file
    };

    // read in the mode to get record length
    let mut header_buffer = [0; 1024 * 64];
    let mut read = match file.read(&mut header_buffer) {
        Err(why) => panic!("Couldn't read header: {}", why.description()),
        Ok(read) => read
    };
    let mode = String::from_utf8_lossy(&header_buffer[0..5]);
    let record_length = if mode == "MODE0" { 28 } else { 32 };

    let total_particles = LittleEndian::read_i32(&header_buffer[5..9]);
    println!("Total particles are {:?}", total_particles);

    // ok so now we want to seek there
    let xy_offset = 8;
    let mut buffer;
    buffer = header_buffer;
    let mut offset = record_length;
    let mut position = 0;
    while read != 0 {
        let number_records = (read - offset) / record_length;
        for i in 0..number_records {
            let index = offset + i * record_length + xy_offset;
            let x = LittleEndian::read_f32(&buffer[index..index+4]);
            let y = LittleEndian::read_f32(&buffer[index+4..index+8]);
            LittleEndian::write_f32(&mut buffer[index..index+4], x + x_translate);
            LittleEndian::write_f32(&mut buffer[index+4..index+8], y + y_translate);
        }
        offset = (read - offset) % record_length;
        position = match file.seek(SeekFrom::Start(position)) {
            Err(why) => panic!("Could not seek back for write: {}", why.description()),
            Ok(position) => position
        };
        match file.write(&buffer[0..read]) {
            Err(why) => panic!("Could not write: {}", why.description()),
            Ok(pos) => pos
        };
        position += read as u64;
        read = match file.read(&mut buffer) {
            Err(why) => panic!("Couldn't read more: {}", why.description()),
            Ok(read) => read
        };
    }
}
