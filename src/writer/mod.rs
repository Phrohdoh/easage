use ::std::path::{Path, PathBuf};
use ::std::io::{self, BufWriter, Write};
use ::std::fs::{File, OpenOptions};

use walkdir::WalkDir;
use byteorder::{BigEndian, LittleEndian, WriteBytesExt};

struct Entry {
    name: PathBuf,
    len: u32,
}

impl Entry {
    pub fn new(name: PathBuf, len: u32) -> Self {
        Self { name, len }
    }
}

pub fn pack_directory<P1, P2>(input_directory: P1, output_filepath: P2, kind: ::Kind, secret_data: Option<&[u8]>) -> ::std::io::Result<()> 
    where P1: AsRef<Path>,
          P2: AsRef<Path> {
    let input_directory = input_directory.as_ref();
    let output_filepath = output_filepath.as_ref();

    let mut entries = vec![];

    for entry in WalkDir::new(input_directory) {
        let entry = entry.unwrap();

        let md = entry.metadata().unwrap();
        if md.is_dir() {
            continue;
        }

        let path = entry.path().to_path_buf();
        entries.push(Entry::new(path, md.len() as u32));
    }

    entries.sort_by(|a, b| a.len.cmp(&b.len));

    let table_size = calc_table_size(entries.iter());
    let data_start = ::Archive::HEADER_LEN + table_size + secret_data.map(|data| data.len()).unwrap_or(0) as u32;

    let kind_bytes = match kind {
        ::Kind::Big4 => "BIG4",
        ::Kind::BigF => "BIGF",
        _ => panic!("TODO: Return an error if called with Kind::Unknown")
    }.as_bytes();

    let buf = Vec::with_capacity(data_start as usize);
    let mut writer = BufWriter::new(buf);

    // Write the header
    writer.write(kind_bytes).expect("Failed to write format bytes");
    writer.write_u32::<LittleEndian>(0).expect("Failed to write [bogus] size"); // TODO
    writer.write_u32::<BigEndian>(entries.len() as u32).expect("Failed to write len");
    writer.write_u32::<BigEndian>(data_start).expect("Failed to write data_start");

    // Write the entry metadata table
    let mut last_len = 0u32;
    for entry in &entries {
        let len = entry.len;
        let offset = data_start + last_len;
        let name_bytes = entry.name.to_str().unwrap().as_bytes();

        writer.write_u32::<BigEndian>(offset).expect("Failed to write entry's offset");
        writer.write_u32::<BigEndian>(len as u32).expect("Failed to write entry's len");
        writer.write(name_bytes).expect("Failed to write entry's name");
        writer.write(&[b'\0']).expect("Failed to write entry's name's trailing NUL");

        last_len = len;
    }

    if let Some(secret_data) = secret_data {
        writer.write(secret_data).expect("Failed to write secret data block between table and contents");
    }

    // Write the actual data
    for entry in entries {
        let mut f = File::open(entry.name).unwrap();
        io::copy(&mut f, &mut writer).expect("Failed to write entry's data");
    }

    let inner = writer.into_inner().unwrap();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(output_filepath)
        .unwrap();

    file.write_all(&inner).expect(&format!("Failed to write {}", output_filepath.display()));

    Ok(())
}

fn calc_table_size<'e, I: Iterator<Item=&'e Entry>>(entries: I) -> u32 {
    entries.map(table_record_size).sum()
}

fn table_record_size(e: &Entry) -> u32 {
    (::std::mem::size_of::<u32>() + // offset
     ::std::mem::size_of::<u32>() + // length
     e.name.to_str().unwrap().len() + 1) as u32 // name + null
}