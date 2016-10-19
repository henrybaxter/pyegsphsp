extern crate byteorder;

use std::error::Error;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::SeekFrom;
use std::io::Cursor;

fn main() {
    // get the input path
    let path = Path::new("input.egsphsp2");
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
    let mut mode_buffer = [0; 5];
    match file.read_exact(&mut mode_buffer) {
        Err(why) => panic!("Couldn't read mode: {}", why.description()),
        Ok(read) => read
    }
    let mode = String::from_utf8_lossy(&mode_buffer);
    let record_length = if mode == "MODE0" { 28 } else { 32 };

    let mut total_particles_buffer = [0; 4];
    match file.read_exact(&mut total_particles_buffer) {
        Err(why) => panic!("Couldn't read number of particles: {}", why.description()),
        Ok(read) => read
    }
    file.seek(SeekFrom::Start(record_length)); // pass header
    //let total_particles = mem::transmute::<[u8; 4], i32>(total_particles_buffer);
    let total_particles = LittleEndian::read_i32(&total_particles_buffer);
    println!("Total particles are {:?}", total_particles);
    println!("Total particles are {:?}", total_particles as u64);

    // ok so now we want to seek there
    let xy_offset = 8;
    let mut buffer = [0; 4096];
    let mut offset = 0;
    loop {
        let read = match file.read(&mut buffer) {
            Err(why) => panic!("Couldn't read number of particles: {}", why.description()),
            Ok(read) => read
        };
        if read == 0 {
            break;
        }
        // ok loop along this buffer, seeking and reading and writing
        // how long is it? read is the number of bytes
        // we want to know how many records, so
        let buffer_cursor = Cursor::new(buffer);
        let number_records = read / record_length;
        for i in 0..number_records {
            let index = offset + i * record_length + xy_offset;
            buffer_cursor.seek(SeekFrom::Start(index as u64));
            let x = buffer_cursor.read_f32::<LittleEndian>().unwrap();
            let y = buffer_cursor.read_f32::<LittleEndian>().unwrap();
            buffer_cursor.seek(SeekFrom::Start(index as u64));
            buffer_cursor.write_f32::<LittleEndian>(x + x_translate);
            buffer_cursor.write_f32::<LittleEndian>(y + y_translate);
        }
        offset = read % record_length;
    }
}
